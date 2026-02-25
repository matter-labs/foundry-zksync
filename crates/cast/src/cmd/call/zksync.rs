use alloy_json_abi::Function;
use alloy_primitives::{TxKind, U256};
use alloy_provider::Provider;
use alloy_rpc_types::{BlockId, TransactionRequest};
use alloy_serde::WithOtherFields;
use alloy_sol_types::SolCall;
use alloy_zksync::network::transaction_request::TransactionRequest as ZkTransactionRequest;
use crate::{Cast, ZkCast, ZkTransactionOpts};
use eyre::Result;
use foundry_cli::utils;
use foundry_config::Config;

use alloy_network::AnyNetwork;

/// Execute a zksync call, returning the response string.
pub async fn run_zk_call<P: Provider<AnyNetwork>>(
    provider: &P,
    config: &Config,
    tx: &WithOtherFields<TransactionRequest>,
    func: Option<Function>,
    zk_tx: ZkTransactionOpts,
    zkcode: Option<String>,
    block: Option<BlockId>,
) -> Result<String> {
    let func = func.map(|func| convert_func(tx, func)).transpose()?;
    let zk_tx = convert_tx(tx.clone(), zk_tx, zkcode).await?;

    let cast = Cast::new(provider);
    let zk_cast = ZkCast::new(utils::get_provider_zksync(config)?, cast);

    zk_cast.call_zk(&zk_tx, func.as_ref(), block).await
}

/// Converts the given tx request to be a full ZkSync transaction request with fee estimation
async fn convert_tx(
    evm_tx: WithOtherFields<TransactionRequest>,
    zk_tx: ZkTransactionOpts,
    zk_code: Option<String>,
) -> Result<ZkTransactionRequest> {
    let mut tx = zk_tx.build_base_tx(evm_tx, zk_code)?;

    // NOTE(zk): here we are doing a `call` so the fee doesn't matter
    // but we need a valid value for `gas_per_pubdata`
    tx.set_gas_per_pubdata(U256::from(50_000));

    Ok(tx)
}

/// Retrieve the appropriate function given the transaction options
fn convert_func(tx: &WithOtherFields<TransactionRequest>, func: Function) -> Result<Function> {
    // if we are deploying we should return the "create" function
    // instead of the original which may be the constructor
    if tx.to == Some(TxKind::Create) {
        Function::parse(alloy_zksync::contracts::l2::contract_deployer::createCall::SIGNATURE)
            .map_err(Into::into)
    } else {
        Ok(func)
    }
}
