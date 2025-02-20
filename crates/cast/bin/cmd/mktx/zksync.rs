use alloy_network::TransactionBuilder;
use alloy_rpc_types::TransactionRequest;
use alloy_serde::WithOtherFields;
use alloy_zksync::{
    network::transaction_request::TransactionRequest as ZkTransactionRequest,
    provider::ZksyncProvider,
};
use cast::ZkTransactionOpts;
use eyre::Result;
use foundry_cli::utils;
use foundry_config::Config;

/// Builds a complete ZkSync transaction request with fee estimation
pub async fn build_tx(
    zk_tx: ZkTransactionOpts,
    evm_tx: WithOtherFields<TransactionRequest>,
    zk_code: String,
    config: &Config,
) -> Result<ZkTransactionRequest> {
    let zk_provider = utils::get_provider_zksync(config)?;
    let mut tx = zk_tx.build_base_tx(evm_tx, Some(zk_code))?;

    let fee = ZksyncProvider::estimate_fee(&zk_provider, tx.clone()).await?;
    tx.set_max_fee_per_gas(fee.max_fee_per_gas);
    tx.set_max_priority_fee_per_gas(fee.max_priority_fee_per_gas);
    tx.set_gas_limit(fee.gas_limit);

    Ok(tx)
}
