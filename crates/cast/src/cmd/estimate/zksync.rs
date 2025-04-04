use alloy_provider::Provider;
use alloy_rpc_types::TransactionRequest;
use alloy_serde::WithOtherFields;
use cast::ZkTransactionOpts;
use eyre::Result;
use foundry_cli::utils;
use foundry_config::Config;
/// Estimates gas for a ZkSync transaction
pub async fn estimate_gas(
    zk_tx: ZkTransactionOpts,
    evm_tx: WithOtherFields<TransactionRequest>,
    zk_code: Option<String>,
    config: &Config,
) -> Result<u64> {
    let zk_provider = utils::get_provider_zksync(config)?;
    let tx = zk_tx.build_base_tx(evm_tx, zk_code)?;
    Ok(zk_provider.estimate_gas(tx).await?)
}
