//! # foundry-zksync
//!
//! Main Foundry ZKSync implementation.
#![warn(missing_docs, unused_crate_dependencies)]

/// Contains cheatcode implementations.
pub mod cheatcodes;

/// Contains conversion utils for revm primitives.
pub mod convert;

/// Contains zksync utils.
pub mod utils;

/// ZKSync Era VM implementation.
pub mod vm;

/// ZKSync Era State implementation.
pub mod state;

use alloy_primitives::{Address, Bytes, U256 as rU256};
use convert::{ConvertAddress, ConvertH256, ConvertU256};
use eyre::{eyre, OptionExt};
pub use utils::{fix_l2_gas_limit, fix_l2_gas_price};
pub use vm::{balance, encode_create_params, nonce};

use zksync_types::utils::storage_key_for_eth_balance;
pub use zksync_types::{
    ACCOUNT_CODE_STORAGE_ADDRESS, CONTRACT_DEPLOYER_ADDRESS, L2_ETH_TOKEN_ADDRESS,
    NONCE_HOLDER_ADDRESS,
};
use zksync_web3_rs::{
    eip712::{Eip712Meta, Eip712Transaction, Eip712TransactionRequest},
    providers::Middleware,
    signers::Signer,
    types::transaction::eip2718::TypedTransaction,
    zks_provider::types::Fee,
    zks_utils::EIP712_TX_TYPE,
};

type Result<T> = std::result::Result<T, eyre::Report>;

/// Returns the balance key for a provided account address.
pub fn get_balance_key(address: Address) -> rU256 {
    storage_key_for_eth_balance(&address.to_h160()).key().to_ru256()
}

/// Represents additional data for ZK transactions.
#[derive(Clone, Debug, Default)]
pub struct ZkTransactionMetadata {
    /// Factory Deps for ZK transactions.
    pub factory_deps: Vec<Vec<u8>>,
}

/// Creates a new signed EIP-712 transaction with the provided factory deps.
pub async fn new_eip712_transaction<M: Middleware, S: Signer>(
    legacy_or_1559: TypedTransaction,
    factory_deps: Vec<Vec<u8>>,
    provider: M,
    signer: S,
) -> Result<Bytes> {
    let from = legacy_or_1559.from().cloned().ok_or_eyre("`from` cannot be empty")?;
    let to = legacy_or_1559
        .to()
        .and_then(|to| to.as_address())
        .cloned()
        .ok_or_eyre("`to` cannot be empty")?;
    let chain_id = legacy_or_1559.chain_id().ok_or_eyre("`chain_id` cannot be empty")?;
    let nonce = legacy_or_1559.nonce().ok_or_eyre("`nonce` cannot be empty")?;
    let gas_price = legacy_or_1559.gas_price().ok_or_eyre("`gas_price` cannot be empty")?;
    let max_cost = legacy_or_1559.max_cost().ok_or_eyre("`max_cost` cannot be empty")?;
    let data = legacy_or_1559.data().cloned().ok_or_eyre("`data` cannot be empty")?;
    let custom_data = Eip712Meta::new().factory_deps(factory_deps);

    let mut deploy_request = Eip712TransactionRequest::new()
        .r#type(EIP712_TX_TYPE)
        .from(from)
        .to(to)
        .chain_id(chain_id.as_u64())
        .nonce(nonce)
        .gas_price(gas_price)
        .max_fee_per_gas(max_cost)
        .data(data)
        .custom_data(custom_data);

    let gas_price = provider
        .get_gas_price()
        .await
        .map_err(|err| eyre!("failed retrieving gas_price {:?}", err))?;
    let fee: Fee = provider
        .provider()
        .request("zks_estimateFee", [deploy_request.clone()])
        .await
        .map_err(|err| eyre!("failed estimating fee {:?}", err))?;
    deploy_request = deploy_request
        .gas_limit(fee.gas_limit)
        .max_fee_per_gas(fee.max_fee_per_gas)
        .max_priority_fee_per_gas(fee.max_priority_fee_per_gas)
        .gas_price(gas_price);

    let signable: Eip712Transaction = deploy_request
        .clone()
        .try_into()
        .map_err(|err| eyre!("failed converting deploy request to eip-712 tx {:?}", err))?;

    let signature = signer.sign_typed_data(&signable).await.expect("Failed to sign typed data");
    let encoded_rlp = deploy_request
        .rlp_signed(signature)
        .map_err(|err| eyre!("failed encoding deployment request {:?}", err))?;

    let tx = [&[EIP712_TX_TYPE], encoded_rlp.to_vec().as_slice()].concat().into();

    Ok(tx)
}

/// Estimated gas from a ZK network.
pub struct EstimatedGas {
    /// Estimated gas price.
    pub price: rU256,
    /// Estimated gas limit.
    pub limit: rU256,
}

/// Estimates the gas parameters for the provided transaction.
pub async fn estimate_gas<M: Middleware>(
    legacy_or_1559: &TypedTransaction,
    factory_deps: Vec<Vec<u8>>,
    provider: M,
) -> Result<EstimatedGas> {
    let to = legacy_or_1559
        .to()
        .and_then(|to| to.as_address())
        .cloned()
        .ok_or_eyre("`to` cannot be empty")?;
    let chain_id = legacy_or_1559.chain_id().ok_or_eyre("`chain_id` cannot be empty")?;
    let nonce = legacy_or_1559.nonce().ok_or_eyre("`nonce` cannot be empty")?;
    let gas_price = legacy_or_1559.gas_price().ok_or_eyre("`gas_price` cannot be empty")?;
    let data = legacy_or_1559.data().cloned().ok_or_eyre("`data` cannot be empty")?;
    let custom_data = Eip712Meta::new().factory_deps(factory_deps);

    let mut deploy_request = Eip712TransactionRequest::new()
        .r#type(EIP712_TX_TYPE)
        .to(to)
        .chain_id(chain_id.as_u64())
        .nonce(nonce)
        .gas_price(gas_price)
        .data(data)
        .custom_data(custom_data);
    if let Some(from) = legacy_or_1559.from() {
        deploy_request = deploy_request.from(*from)
    }

    let gas_price = provider.get_gas_price().await.unwrap();
    let fee: Fee = provider
        .provider()
        .request("zks_estimateFee", [deploy_request.clone()])
        .await
        .map_err(|err| eyre!("failed rpc call for estimating fee: {:?}", err))?;

    Ok(EstimatedGas { price: gas_price.to_ru256(), limit: fee.gas_limit.to_ru256() })
}
