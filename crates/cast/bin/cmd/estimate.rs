use crate::tx::{CastTxBuilder, SenderKind};
use alloy_primitives::U256;
use alloy_provider::Provider;
use alloy_rpc_types::BlockId;
use cast::zksync::ZkTransactionOpts;
use clap::Parser;
use eyre::Result;
use foundry_cli::{
    opts::{EthereumOpts, TransactionOpts},
    utils::{self, parse_ether_value, LoadConfig},
};
use foundry_common::ens::NameOrAddress;
use std::str::FromStr;

mod zksync;

/// CLI arguments for `cast estimate`.
#[derive(Debug, Parser)]
pub struct EstimateArgs {
    /// The destination of the transaction.
    #[arg(value_parser = NameOrAddress::from_str)]
    to: Option<NameOrAddress>,

    /// The signature of the function to call.
    sig: Option<String>,

    /// The arguments of the function to call.
    args: Vec<String>,

    /// The block height to query at.
    ///
    /// Can also be the tags earliest, finalized, safe, latest, or pending.
    #[arg(long, short = 'B')]
    block: Option<BlockId>,

    #[command(subcommand)]
    command: Option<EstimateSubcommands>,

    #[command(flatten)]
    tx: TransactionOpts,

    #[command(flatten)]
    eth: EthereumOpts,

    /// Zksync Transaction
    #[command(flatten)]
    zk_tx: ZkTransactionOpts,

    /// Force a zksync eip-712 transaction and apply CREATE overrides
    #[arg(long = "zksync")]
    zk_force: bool,
}

#[derive(Debug, Parser)]
pub enum EstimateSubcommands {
    /// Estimate gas cost to deploy a smart contract
    #[command(name = "--create")]
    Create {
        /// The bytecode of contract
        code: String,

        /// The signature of the constructor
        sig: Option<String>,

        /// Constructor arguments
        args: Vec<String>,

        /// Ether to send in the transaction
        ///
        /// Either specified in wei, or as a string with a unit type:
        ///
        /// Examples: 1ether, 10gwei, 0.01ether
        #[arg(long, value_parser = parse_ether_value)]
        value: Option<U256>,
    },
}

impl EstimateArgs {
    pub async fn run(self) -> Result<()> {
        let Self { to, mut sig, mut args, mut tx, block, eth, command, zk_tx, zk_force } = self;

        let config = eth.load_config()?;
        let provider = utils::get_provider(&config)?;
        let sender = SenderKind::from_wallet_opts(eth.wallet).await?;

        let code = if let Some(EstimateSubcommands::Create {
            code,
            sig: create_sig,
            args: create_args,
            value,
        }) = command
        {
            sig = create_sig;
            args = create_args;
            if let Some(value) = value {
                tx.value = Some(value);
            }
            Some(code)
        } else {
            None
        };

        let (tx, _) = CastTxBuilder::new(&provider, tx, &config)
            .await?
            .with_to(to)
            .await?
            // NOTE(zk): `with_code_sig_and_args` decodes the code and appends it to the input
            // we want the raw decoded constructor input from that function so we keep the code
            // to encode the CONTRACT_CREATOR call later
            .with_code_sig_and_args(code.clone(), sig, args)
            .await?
            .build_raw(sender)
            .await?;

        let gas = if zk_tx.has_zksync_args() || zk_force {
            zksync::estimate_gas(zk_tx, tx, code, &config).await?
        } else {
            provider.estimate_gas(tx).block(block.unwrap_or_default()).await?
        };

        sh_println!("{gas}")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_estimate_value() {
        let args: EstimateArgs = EstimateArgs::parse_from(["foundry-cli", "--value", "100"]);
        assert!(args.tx.value.is_some());
    }
}
