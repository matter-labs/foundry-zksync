use alloy_network::TransactionBuilder;
use alloy_primitives::{hex, Address, Bytes, TxKind, U256};
use alloy_rpc_types::TransactionRequest;
use alloy_serde::WithOtherFields;
use alloy_zksync::network::{
    transaction_request::TransactionRequest as ZkTransactionRequest,
    unsigned_tx::eip712::PaymasterParams,
};
use clap::{command, Parser};
use eyre::Result;
use foundry_cli::utils;
use foundry_config::Config;

#[derive(Clone, Debug, Parser)]
#[command(next_help_heading = "Transaction options")]
pub struct ZkTransactionOpts {
    /// Paymaster address for the ZKSync transaction
    #[arg(long = "zk-paymaster-address", requires = "paymaster_input")]
    pub paymaster_address: Option<Address>,

    /// Paymaster input for the ZKSync transaction
    #[arg(long = "zk-paymaster-input", requires = "paymaster_address", value_parser = parse_hex_bytes)]
    pub paymaster_input: Option<Bytes>,

    /// Factory dependencies for the ZKSync transaction
    #[arg(long = "zk-factory-deps", value_parser = parse_hex_bytes, value_delimiter = ',')]
    pub factory_deps: Vec<Bytes>,

    /// Gas per pubdata for the ZKSync transaction
    #[arg(long = "zk-gas-per-pubdata")]
    pub gas_per_pubdata: Option<U256>,
}

fn parse_hex_bytes(s: &str) -> Result<Bytes, String> {
    hex::decode(s).map(Bytes::from).map_err(|e| format!("Invalid hex string: {e}"))
}

impl ZkTransactionOpts {
    pub fn has_zksync_args(&self) -> bool {
        self.paymaster_address.is_some() ||
            !self.factory_deps.is_empty() ||
            self.gas_per_pubdata.is_some()
    }
}

pub async fn build_tx(
    tx: WithOtherFields<TransactionRequest>,
    zk_tx: ZkTransactionOpts,
    config: &Config,
    zk_code: String,
) -> Result<ZkTransactionRequest> {
    let zk_provider = utils::get_provider_zksync(config)?;
    let is_create = tx.to == Some(TxKind::Create);
    let mut tx: ZkTransactionRequest = tx.inner.into();

    if let Some(gas_per_pubdata) = zk_tx.gas_per_pubdata {
        tx.set_gas_per_pubdata(gas_per_pubdata);
    }

    if let (Some(paymaster), Some(paymaster_input)) =
        (zk_tx.paymaster_address, zk_tx.paymaster_input)
    {
        tx.set_paymaster_params(PaymasterParams { paymaster, paymaster_input });
    }

    if is_create {
        let input_data = tx.input().cloned().unwrap_or_default().to_vec();
        let zk_code_bytes = hex::decode(zk_code)?;
        let constructor_args = &input_data[zk_code_bytes.len()..];

        tx = tx.with_create_params(
            zk_code_bytes,
            constructor_args.to_vec(),
            zk_tx.factory_deps.into_iter().map(|b| b.to_vec()).collect(),
        )?;
    } else {
        tx.set_factory_deps(zk_tx.factory_deps);
    }
    tx.prep_for_submission();

    let fee =
        alloy_zksync::provider::ZksyncProvider::estimate_fee(&zk_provider, tx.clone()).await?;

    tx.set_max_fee_per_gas(fee.max_fee_per_gas);
    tx.set_max_priority_fee_per_gas(fee.max_priority_fee_per_gas);
    tx.set_gas_limit(fee.gas_limit);
    Ok(tx)
}
