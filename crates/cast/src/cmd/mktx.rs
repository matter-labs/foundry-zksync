use crate::{
    tx::{self, CastTxBuilder, SenderKind},
    zksync::{NoopWallet, ZkTransactionOpts},
};
use alloy_ens::NameOrAddress;
use alloy_network::{EthereumWallet, TransactionBuilder, eip2718::Encodable2718};
use alloy_primitives::hex;
use alloy_provider::Provider;
use alloy_signer::Signer;
use alloy_zksync::wallet::ZksyncWallet;
use clap::Parser;
use eyre::{OptionExt, Result};
use foundry_cli::{
    opts::{EthereumOpts, TransactionOpts},
    utils::{LoadConfig, get_provider},
};
use std::{path::PathBuf, str::FromStr};

mod zksync;
use zksync::build_tx;

/// CLI arguments for `cast mktx`.
#[derive(Debug, Parser)]
pub struct MakeTxArgs {
    /// The destination of the transaction.
    ///
    /// If not provided, you must use `cast mktx --create`.
    #[arg(value_parser = NameOrAddress::from_str)]
    to: Option<NameOrAddress>,

    /// The signature of the function to call.
    sig: Option<String>,

    /// The arguments of the function to call.
    args: Vec<String>,

    #[command(subcommand)]
    command: Option<MakeTxSubcommands>,

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

    #[command(flatten)]
    eth: EthereumOpts,
    /// Zksync Transaction
    #[command(flatten)]
    zk_tx: ZkTransactionOpts,

    /// Force a zksync eip-712 transaction and apply CREATE overrides
    #[arg(long = "zksync")]
    zk_force: bool,

    /// Generate a raw RLP-encoded unsigned transaction.
    ///
    /// Relaxes the wallet requirement.
    #[arg(long, requires = "from")]
    raw_unsigned: bool,

    /// Call `eth_signTransaction` using the `--from` argument or $ETH_FROM as sender
    #[arg(long, requires = "from", conflicts_with = "raw_unsigned")]
    ethsign: bool,
}

#[derive(Debug, Parser)]
pub enum MakeTxSubcommands {
    /// Use to deploy raw contract bytecode.
    #[command(name = "--create")]
    Create {
        /// The initialization bytecode of the contract to deploy.
        code: String,

        /// The signature of the constructor.
        sig: Option<String>,

        /// The constructor arguments.
        args: Vec<String>,
    },
}

impl MakeTxArgs {
    pub async fn run(self) -> Result<()> {
        let Self {
            to,
            mut sig,
            mut args,
            command,
            tx,
            path,
            eth,
            zk_tx,
            zk_force,
            raw_unsigned,
            ethsign,
        } = self;

        let blob_data = if let Some(path) = path { Some(std::fs::read(path)?) } else { None };

        let mut zkcode = Default::default();
        let code = if let Some(MakeTxSubcommands::Create {
            code,
            sig: constructor_sig,
            args: constructor_args,
        }) = command
        {
            zkcode = code.clone();
            sig = constructor_sig;
            args = constructor_args;
            Some(code)
        } else {
            None
        };

        let config = eth.load_config()?;

        let provider = get_provider(&config)?;

        // NOTE(zk): tx is built in two steps as signer might have a different type
        let tx_builder = CastTxBuilder::new(&provider, tx, &config)
            .await?
            .with_to(to)
            .await?
            .with_code_sig_and_args(code, sig, args)
            .await?
            .with_blob_data(blob_data)?;

        if raw_unsigned {
            // Build unsigned raw tx
            let from = eth.wallet.from.ok_or_eyre("missing `--from` address")?;
            let raw_tx = tx_builder.build_unsigned_raw(from).await?;

            sh_println!("{raw_tx}")?;
            return Ok(());
        }

        if ethsign {
            // Use "eth_signTransaction" to sign the transaction only works if the node/RPC has
            // unlocked accounts.
            let (tx, _) = tx_builder.build(config.sender).await?;
            let signed_tx = provider.sign_transaction(tx).await?;

            sh_println!("{signed_tx}")?;
            return Ok(());
        }

        // NOTE(zk): if custom signature is sent, signer is not used so
        // we do not bail in that case, the Result is kept instead
        let (from, maybe_signer) = if zk_tx.custom_signature.is_some() {
            if let Some(from) = eth.wallet.from {
                (from, None)
            } else {
                eyre::bail!("expected address via --from option to be used for custom signature");
            }
        } else {
            // Default to using the local signer.
            // Get the signer from the wallet, and fail if it can't be constructed.
            let signer = eth.wallet.signer().await?;
            let from = signer.address();

            tx::validate_from_address(eth.wallet.from, from)?;

            (from, Some(signer))
        };

        let (tx, _) = if zk_tx.custom_signature.is_some() {
            tx_builder.build_raw(SenderKind::Address(from)).await?
        } else {
            tx_builder.build_raw(maybe_signer.as_ref().expect("No signer found")).await?
        };

        if zk_tx.has_zksync_args() || zk_force {
            let zktx = build_tx(zk_tx, tx, zkcode, &config).await?;

            let signed_tx = if zktx.custom_signature().is_some() {
                let zk_wallet = NoopWallet { address: from };
                zktx.build(&zk_wallet).await?.encoded_2718()
            } else {
                let zk_wallet = ZksyncWallet::new(maybe_signer.expect("No signer found"));
                zktx.build(&zk_wallet).await?.encoded_2718()
            };

            sh_println!("0x{}", hex::encode(signed_tx))?;

            Ok(())
        } else {
            let signer = maybe_signer.expect("No signer found");
            let tx = tx.build(&EthereumWallet::new(signer)).await?;

            let signed_tx = hex::encode(tx.encoded_2718());

            sh_println!("0x{signed_tx}")?;

            Ok(())
        }
    }
}
