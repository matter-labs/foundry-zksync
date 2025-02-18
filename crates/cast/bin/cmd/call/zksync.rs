use alloy_json_abi::Function;
use alloy_network::TransactionBuilder;
use alloy_primitives::{TxKind, U256};
use alloy_rpc_types::TransactionRequest;
use alloy_serde::WithOtherFields;
use alloy_sol_types::SolCall;
use alloy_zksync::{
    network::transaction_request::TransactionRequest as ZkTransactionRequest, provider::ZksyncProvider
};
use cast::ZkTransactionOpts;
use eyre::Result;
use foundry_cli::utils;
use foundry_config::Config;

/// Converts the given tx request to be a full ZkSync transaction request with fee estimation
pub async fn convert_tx(
    evm_tx: WithOtherFields<TransactionRequest>,
    zk_tx: ZkTransactionOpts,
    zk_code: Option<String>,
    config: &Config,
) -> Result<ZkTransactionRequest> {
    let zk_provider = utils::get_provider_zksync(config)?;
    let mut tx = zk_tx.build_base_tx(evm_tx, zk_code)?;

    let Ok(fee) = ZksyncProvider::estimate_fee(&zk_provider, tx.clone()).await else {
        warn!("unable to estimate fee thru `zks_estimateFee` endpoint, transaction may fail");
        tx.set_gas_per_pubdata(U256::from(3000));
        return Ok(tx);
    };

    tx.set_max_fee_per_gas(fee.max_fee_per_gas);
    tx.set_max_priority_fee_per_gas(fee.max_priority_fee_per_gas);
    tx.set_gas_limit(fee.gas_limit);

    Ok(tx)
}

/// Retrieve the appropriate function given the transaction options
pub fn convert_func(tx: &WithOtherFields<TransactionRequest>, func: Function) -> Result<Function> {
    // if we are deploying we should return the "create" function
    // instead of the original which may be the constructor
    if tx.to == Some(TxKind::Create) {
        Function::parse(alloy_zksync::contracts::l2::contract_deployer::createCall::SIGNATURE).map_err(Into::into)}
    else {
        Ok(func)
    }
}
