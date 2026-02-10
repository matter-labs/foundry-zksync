use std::{path::PathBuf, str::FromStr, time::Duration};

use crate::{
    tx::{self, CastTxBuilder, CastTxSender, SendTxOpts},
    zksync::ZkTransactionOpts,
};
use alloy_eips::Encodable2718;
use alloy_ens::NameOrAddress;
use alloy_network::{AnyNetwork, EthereumWallet, TransactionBuilder};
use alloy_primitives::TxHash;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::TransactionRequest;
use alloy_serde::WithOtherFields;
use alloy_signer::Signer;
use clap::Parser;
use eyre::{Result, eyre};
use foundry_cli::{
    opts::TransactionOpts,
    utils::{self, LoadConfig, get_provider_with_curl},
};
use foundry_wallets::WalletSigner;

mod zksync;
use zksync::send_zk_transaction;

/// CLI arguments for `cast send`.
#[derive(Debug, Parser)]
pub struct SendTxArgs {
    /// The destination of the transaction.
    ///
    /// If not provided, you must use cast send --create.
    #[arg(value_parser = NameOrAddress::from_str)]
    to: Option<NameOrAddress>,

    /// The signature of the function to call.
    sig: Option<String>,

    /// The arguments of the function to call.
    #[arg(allow_negative_numbers = true)]
    args: Vec<String>,

    /// Raw hex-encoded data for the transaction. Used instead of \[SIG\] and \[ARGS\].
    #[arg(
        long,
        conflicts_with_all = &["sig", "args"]
    )]
    data: Option<String>,

    #[command(flatten)]
    send_tx: SendTxOpts,

    #[command(subcommand)]
    command: Option<SendTxSubcommands>,

    /// Send via `eth_sendTransaction` using the `--from` argument or $ETH_FROM as sender
    #[arg(long, requires = "from")]
    unlocked: bool,

    #[command(flatten)]
    tx: TransactionOpts,

    /// The path of blob data to be sent.
    #[arg(
        long,
        value_name = "BLOB_DATA_PATH",
        conflicts_with = "legacy",
        requires = "blob",
        help_heading = "Transaction options"
    )]
    path: Option<PathBuf>,

    /// Zksync Transaction
    #[command(flatten)]
    zk_tx: ZkTransactionOpts,

    /// Force a zksync eip-712 transaction and apply CREATE overrides
    #[arg(long = "zksync")]
    zk_force: bool,
}

#[derive(Debug, Parser)]
pub enum SendTxSubcommands {
    /// Use to deploy raw contract bytecode.
    #[command(name = "--create")]
    Create {
        /// The bytecode of the contract to deploy.
        code: String,

        /// The signature of the function to call.
        sig: Option<String>,

        /// The arguments of the function to call.
        #[arg(allow_negative_numbers = true)]
        args: Vec<String>,
    },
}

impl SendTxArgs {
    pub async fn run(self) -> eyre::Result<()> {
        let Self {
            to,
            mut sig,
            mut args,
            data,
            send_tx,
            tx,
            command,
            unlocked,
            path,
            zk_tx,
            zk_force,
        } = self;

        let blob_data = if let Some(path) = path { Some(std::fs::read(path)?) } else { None };

        let mut zk_code = Default::default();

        if let Some(data) = data {
            sig = Some(data);
        }

        let code = if let Some(SendTxSubcommands::Create {
            code,
            sig: constructor_sig,
            args: constructor_args,
        }) = command
        {
            zk_code = Some(code.clone());

            // ensure we don't violate settings for transactions that can't be CREATE: 7702 and 4844
            // which require mandatory target
            if to.is_none() && !tx.auth.is_empty() {
                return Err(eyre!(
                    "EIP-7702 transactions can't be CREATE transactions and require a destination address"
                ));
            }
            // ensure we don't violate settings for transactions that can't be CREATE: 7702 and 4844
            // which require mandatory target
            if to.is_none() && blob_data.is_some() {
                return Err(eyre!(
                    "EIP-4844 transactions can't be CREATE transactions and require a destination address"
                ));
            }

            sig = constructor_sig;
            args = constructor_args;
            Some(code)
        } else {
            None
        };

        let config = send_tx.eth.load_config()?;
        let provider = get_provider_with_curl(&config, send_tx.eth.rpc.curl)?;

        if let Some(interval) = send_tx.poll_interval {
            provider.client().set_poll_interval(Duration::from_secs(interval))
        }

        let builder = CastTxBuilder::new(&provider, tx, &config)
            .await?
            .with_to(to)
            .await?
            .with_code_sig_and_args(code, sig, args)
            .await?
            .with_blob_data(blob_data)?;

        let timeout = send_tx.timeout.unwrap_or(config.transaction_timeout);

        // Check if this is a Tempo transaction - requires special handling for local signing
        let is_tempo = builder.is_tempo();

        // Tempo transactions with browser wallets are not supported
        if is_tempo && send_tx.eth.wallet.browser {
            return Err(eyre!("Tempo transactions are not supported with browser wallets."));
        }

        // Case 1:
        // Default to sending via eth_sendTransaction if the --unlocked flag is passed.
        // This should be the only way this RPC method is used as it requires a local node
        // or remote RPC with unlocked accounts.
        if unlocked && !send_tx.eth.wallet.browser {
            // only check current chain id if it was specified in the config
            if let Some(config_chain) = config.chain {
                let current_chain_id = provider.get_chain_id().await?;
                let config_chain_id = config_chain.id();
                // switch chain if current chain id is not the same as the one specified in the
                // config
                if config_chain_id != current_chain_id {
                    sh_warn!("Switching to chain {}", config_chain)?;
                    provider
                        .raw_request::<_, ()>(
                            "wallet_switchEthereumChain".into(),
                            [serde_json::json!({
                                "chainId": format!("0x{:x}", config_chain_id),
                            })],
                        )
                        .await?;
                }
            }

            let (tx, _) = builder.build(config.sender).await?;

            cast_send(
                provider,
                tx.into_inner(),
                send_tx.cast_async,
                send_tx.sync,
                send_tx.confirmations,
                timeout,
            )
            .await
        // Case 2:
        // An option to use a local signer was provided.
        // If we cannot successfully instantiate a local signer, then we will assume we don't have
        // enough information to sign and we must bail.
        } else {
            // NOTE(zk): Avoid initializing `signer` twice as it will error out with Ledger, so we
            // move the signers to their respective blocks.
            if zk_tx.has_zksync_args() || zk_force {
                let zk_provider = utils::get_provider_zksync(&config)?;
                let tx_hash =
                    send_zk_transaction(zk_provider, builder, &send_tx.eth, zk_tx, zk_code).await?;

                let provider =
                    ProviderBuilder::<_, _, AnyNetwork>::default().connect_provider(&provider);
                let cast = CastTxSender::new(provider);

                handle_transaction_result(
                    &cast,
                    &tx_hash,
                    send_tx.cast_async,
                    send_tx.confirmations,
                    timeout,
                )
                .await
            } else {
                // Retrieve the signer, and bail if it can't be constructed.
                let signer = send_tx.eth.wallet.signer().await?;
                let from = signer.address();

                // Browser wallets work differently as they sign and send the transaction in one
                // step.
                if send_tx.eth.wallet.browser
                    && let WalletSigner::Browser(ref browser_signer) = signer
                {
                    let (tx_request, _) = builder.build(from).await?;
                    let tx_hash = browser_signer
                        .send_transaction_via_browser(tx_request.into_inner().inner)
                        .await?;

                    let provider =
                        ProviderBuilder::<_, _, AnyNetwork>::default().connect_provider(&provider);
                    let cast = CastTxSender::new(provider);

                    return handle_transaction_result(
                        &cast,
                        &tx_hash,
                        send_tx.cast_async,
                        send_tx.confirmations,
                        timeout,
                    )
                    .await;
                }

                tx::validate_from_address(send_tx.eth.wallet.from, from)?;

                // Tempo transactions need to be signed locally and sent as raw transactions
                // because EthereumWallet doesn't understand type 0x76
                // TODO(onbjerg): All of this is a side effect of a few things, most notably that we
                // do not use `FoundryNetwork` and `FoundryTransactionRequest`
                // everywhere, which is downstream of the fact that we use
                // `EthereumWallet` everywhere.
                if is_tempo {
                    let (ftx, _) = builder.build(&signer).await?;

                    let signed_tx = ftx.build(&EthereumWallet::new(signer)).await?;

                    // Encode and send raw
                    let mut raw_tx = Vec::with_capacity(signed_tx.encode_2718_len());
                    signed_tx.encode_2718(&mut raw_tx);

                    let cast = CastTxSender::new(&provider);
                    let pending_tx = cast.send_raw(&raw_tx).await?;
                    let tx_hash = pending_tx.inner().tx_hash();

                    if send_tx.cast_async {
                        sh_println!("{tx_hash:#x}")?;
                    } else if send_tx.sync {
                        // For sync mode, we already have the hash, just wait for receipt
                        let receipt = cast
                            .receipt(
                                format!("{tx_hash:#x}"),
                                None,
                                send_tx.confirmations,
                                Some(timeout),
                                false,
                            )
                            .await?;
                        sh_println!("{receipt}")?;
                    } else {
                        let receipt = cast
                            .receipt(
                                format!("{tx_hash:#x}"),
                                None,
                                send_tx.confirmations,
                                Some(timeout),
                                false,
                            )
                            .await?;
                        sh_println!("{receipt}")?;
                    }

                    return Ok(());
                }

                let (tx, _) = builder.build(&signer).await?;

                let wallet = EthereumWallet::from(signer);
                let provider = ProviderBuilder::<_, _, AnyNetwork>::default()
                    .wallet(wallet)
                    .connect_provider(&provider);

                cast_send(
                    provider,
                    tx.into_inner(),
                    send_tx.cast_async,
                    send_tx.sync,
                    send_tx.confirmations,
                    timeout,
                )
                .await
            }
        }
    }
}

pub(crate) async fn cast_send<P: Provider<AnyNetwork>>(
    provider: P,
    tx: WithOtherFields<TransactionRequest>,
    cast_async: bool,
    sync: bool,
    confs: u64,
    timeout: u64,
) -> Result<()> {
    let cast = CastTxSender::new(&provider);
    if sync {
        let receipt = cast.send_sync(tx).await?;
        sh_println!("{receipt}")?;
        Ok(())
    } else {
        let pending_tx = cast.send(tx.clone()).await?;
        let tx_hash = pending_tx.inner().tx_hash();
        handle_transaction_result(&cast, tx_hash, cast_async, confs, timeout).await
    }
}

async fn handle_transaction_result<P: Provider<AnyNetwork>>(
    cast: &CastTxSender<P>,
    tx_hash: &TxHash,
    cast_async: bool,
    confs: u64,
    timeout: u64,
) -> Result<()> {
    if cast_async {
        sh_println!("{tx_hash:#x}")?;
    } else {
        let receipt =
            cast.receipt(format!("{tx_hash:#x}"), None, confs, Some(timeout), false).await?;
        sh_println!("{receipt}")?;
    }
    Ok(())
}
