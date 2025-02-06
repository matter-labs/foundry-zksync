use alloy_network::TransactionBuilder;
use alloy_primitives::{hex, Address, Bytes, TxKind, U256};
use alloy_provider::Provider;
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

    /// Custom signature for the ZKSync transaction
    #[arg(long = "zk-custom-signature", value_parser = parse_hex_bytes)]
    pub custom_signature: Option<Bytes>,

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
            self.custom_signature.is_some() ||
            self.gas_per_pubdata.is_some()
    }
}

pub async fn estimate_gas(
    zk_tx: ZkTransactionOpts,
    config: &Config,
    evm_tx: WithOtherFields<TransactionRequest>,
    zk_code: String,
) -> Result<u64> {
    let zk_provider = utils::get_provider_zksync(config)?;
    let is_create = evm_tx.to == Some(TxKind::Create);
    let mut tx: ZkTransactionRequest = evm_tx.inner.clone().into();
    if let Some(gas_per_pubdata) = zk_tx.gas_per_pubdata {
        tx.set_gas_per_pubdata(gas_per_pubdata)
    }

    if let Some(custom_signature) = &zk_tx.custom_signature {
        tx.set_custom_signature(custom_signature.clone());
    }

    if let (Some(paymaster), Some(paymaster_input)) =
        (zk_tx.paymaster_address, zk_tx.paymaster_input.clone())
    {
        tx.set_paymaster_params(PaymasterParams { paymaster, paymaster_input });
    }

    if is_create {
        let evm_input: Vec<u8> = tx.input().cloned().map(|bytes| bytes.into()).unwrap_or_default();
        let zk_code_decoded = hex::decode(zk_code)?;
        // constructor input gets appended to the bytecode
        let zk_input = &evm_input[zk_code_decoded.len()..];
        tx = tx.with_create_params(
            zk_code_decoded,
            zk_input.to_vec(),
            zk_tx.factory_deps.into_iter().map(|v| v.into()).collect(),
        )?;
    } else {
        tx.set_factory_deps(zk_tx.factory_deps.clone());
    }

    // TODO: Check if alloy calls this for estimate_gas. If so, we do not need this.
    tx.prep_for_submission();
    Ok(zk_provider.estimate_gas(&tx).await?)
}
