use crate::tx::{self, CastTxBuilder};
use alloy_network::{AnyNetwork, EthereumWallet};
use alloy_primitives::{Address, Bytes, TxHash};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::TransactionRequest;
use alloy_serde::WithOtherFields;
use alloy_signer::Signer;
use alloy_transport::Transport;
use cast::Cast;
use clap::{builder::ArgPredicate, Parser};
use eyre::Result;
use foundry_cli::{
    opts::{EthereumOpts, TransactionOpts},
    utils,
};
use foundry_common::ens::NameOrAddress;
use foundry_config::Config;
use foundry_wallets::WalletSigner;
use foundry_zksync_core::{self, convert::ConvertAddress};
use std::{path::PathBuf, str::FromStr};
use zksync_web3_rs::eip712::PaymasterParams;

/// ZkSync-specific paymaster parameters for transactions
#[derive(Debug, Parser)]
pub struct ZksyncParams {
    /// Use ZKSync
    #[arg(long, default_value_ifs([("paymaster_address", ArgPredicate::IsPresent, "true"),("paymaster_input", ArgPredicate::IsPresent, "true")]))]
    zksync: bool,

    /// The paymaster address for the ZKSync transaction
    #[arg(long = "zk-paymaster-address", requires = "paymaster_input")]
    paymaster_address: Option<String>,

    /// The paymaster input for the ZKSync transaction
    #[arg(long = "zk-paymaster-input", requires = "paymaster_address")]
    paymaster_input: Option<String>,
}

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
    args: Vec<String>,

    /// Only print the transaction hash and exit immediately.
    #[arg(id = "async", long = "async", alias = "cast-async", env = "CAST_ASYNC")]
    cast_async: bool,

    /// The number of confirmations until the receipt is fetched.
    #[arg(long, default_value = "1")]
    confirmations: u64,

    #[command(subcommand)]
    command: Option<SendTxSubcommands>,

    /// Send via `eth_sendTransaction using the `--from` argument or $ETH_FROM as sender
    #[arg(long, requires = "from")]
    unlocked: bool,

    /// Timeout for sending the transaction.
    #[arg(long, env = "ETH_TIMEOUT")]
    pub timeout: Option<u64>,

    #[command(flatten)]
    tx: TransactionOpts,

    #[command(flatten)]
    eth: EthereumOpts,

    /// The path of blob data to be sent.
    #[arg(
        long,
        value_name = "BLOB_DATA_PATH",
        conflicts_with = "legacy",
        requires = "blob",
        help_heading = "Transaction options"
    )]
    path: Option<PathBuf>,

    #[command(flatten)]
    zksync_params: ZksyncParams,
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
        args: Vec<String>,
    },
}

impl SendTxArgs {
    #[allow(unknown_lints, dependency_on_unit_never_type_fallback)]
    pub async fn run(self) -> Result<(), eyre::Report> {
        let Self {
            eth,
            to,
            mut sig,
            cast_async,
            mut args,
            tx,
            confirmations,
            command,
            unlocked,
            path,
            timeout,
            zksync_params,
        } = self;

        let blob_data = if let Some(path) = path { Some(std::fs::read(path)?) } else { None };

        let code = if let Some(SendTxSubcommands::Create {
            code,
            sig: constructor_sig,
            args: constructor_args,
        }) = command
        {
            sig = constructor_sig;
            args = constructor_args;
            Some(code)
        } else {
            None
        };

        let mut config = Config::from(&eth);
        config.zksync.startup = zksync_params.zksync;
        config.zksync.compile = zksync_params.zksync;

        let provider = utils::get_provider(&config)?;

        let builder = CastTxBuilder::new(&provider, tx, &config)
            .await?
            .with_to(to)
            .await?
            .with_code_sig_and_args(code, sig, args)
            .await?
            .with_blob_data(blob_data)?;

        let timeout = timeout.unwrap_or(config.transaction_timeout);

        // Case 1:
        // Default to sending via eth_sendTransaction if the --unlocked flag is passed.
        // This should be the only way this RPC method is used as it requires a local node
        // or remote RPC with unlocked accounts.
        if unlocked {
            // only check current chain id if it was specified in the config
            if let Some(config_chain) = config.chain {
                let current_chain_id = provider.get_chain_id().await?;
                let config_chain_id = config_chain.id();
                // switch chain if current chain id is not the same as the one specified in the
                // config
                if config_chain_id != current_chain_id {
                    sh_warn!("Switching to chain {}", config_chain)?;
                    provider
                        .raw_request(
                            "wallet_switchEthereumChain".into(),
                            [serde_json::json!({
                                "chainId": format!("0x{:x}", config_chain_id),
                            })],
                        )
                        .await?;
                }
            }

            let (tx, _) = builder.build(config.sender).await?;

            cast_send(provider, tx, cast_async, confirmations, timeout).await
        // Case 2:
        // An option to use a local signer was provided.
        // If we cannot successfully instantiate a local signer, then we will assume we don't have
        // enough information to sign and we must bail.
        } else {
            // Retrieve the signer, and bail if it can't be constructed.
            let signer = eth.wallet.signer().await?;
            let from = signer.address();

            tx::validate_from_address(eth.wallet.from, from)?;

            if zksync_params.zksync {
                let (tx, _) = builder.build(&signer).await?;
                cast_send_zk(
                    &provider,
                    zksync_params,
                    tx,
                    cast_async,
                    confirmations,
                    timeout,
                    signer,
                )
                .await
            } else {
                // Standard transaction
                let (tx, _) = builder.build(&signer).await?;

                let wallet = EthereumWallet::from(signer);
                let provider = ProviderBuilder::<_, _, AnyNetwork>::default()
                    .wallet(wallet)
                    .on_provider(&provider);

                cast_send(provider, tx, cast_async, confirmations, timeout).await
            }
        }
    }
}

async fn cast_send<P: Provider<T, AnyNetwork>, T: Transport + Clone>(
    provider: P,
    tx: WithOtherFields<TransactionRequest>,
    cast_async: bool,
    confs: u64,
    timeout: u64,
) -> Result<()> {
    let cast = Cast::new(provider);
    let pending_tx = cast.send(tx).await?;

    let tx_hash = pending_tx.inner().tx_hash();

    handle_transaction_result(&cast, tx_hash, cast_async, confs, timeout).await
}

#[allow(clippy::too_many_arguments)]
async fn cast_send_zk<P: Provider<T, AnyNetwork>, T: Transport + Clone>(
    provider: P,
    zksync_params: ZksyncParams,
    tx: WithOtherFields<TransactionRequest>,
    cast_async: bool,
    confs: u64,
    timeout: u64,
    signer: WalletSigner,
) -> Result<()> {
    // ZkSync transaction
    let paymaster_params = zksync_params
        .paymaster_address
        .and_then(|addr| zksync_params.paymaster_input.map(|input| (addr, input)))
        .map(|(addr, input)| PaymasterParams {
            paymaster: Address::from_str(&addr).expect("Invalid paymaster address").to_h160(),
            paymaster_input: Bytes::from_str(&input).expect("Invalid paymaster input").to_vec(),
        });

    // Build EIP712 transaction for ZKSync
    let tx = foundry_zksync_core::new_eip712_transaction(
        tx,
        Vec::new(), // Empty factory_deps
        paymaster_params,
        &provider,
        signer,
    )
    .await
    .map_err(|e| eyre::eyre!("Failed to create EIP712 transaction: {}", e))?;

    // Use send_raw_transaction for ZKSync
    let tx_hash = provider.send_raw_transaction(&tx).await?.tx_hash().to_owned();
    let cast = Cast::new(provider);
    handle_transaction_result(&cast, &tx_hash, cast_async, confs, timeout).await
}

async fn handle_transaction_result<P: Provider<T, AnyNetwork>, T: Transport + Clone>(
    cast: &Cast<P, T>,
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
