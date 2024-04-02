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
use convert::{ConvertAddress, ConvertH256};
use ethers_providers::JsonRpcClient;
use foundry_common::{provider::alloy::try_get_http_provider, runtime_client::RuntimeClient};
pub use utils::{fix_l2_gas_limit, fix_l2_gas_price};
pub use vm::{balance, encode_create_params, nonce};

use zksync_types::utils::storage_key_for_eth_balance;
pub use zksync_types::{
    ACCOUNT_CODE_STORAGE_ADDRESS, CONTRACT_DEPLOYER_ADDRESS, L2_ETH_TOKEN_ADDRESS,
    NONCE_HOLDER_ADDRESS,
};
use zksync_web3_rs::{
    eip712::{Eip712Meta, Eip712Transaction, Eip712TransactionRequest},
    providers::{Middleware, Provider},
    signers::Signer,
    types::transaction::eip2718::TypedTransaction,
    zks_utils::EIP712_TX_TYPE,
};

pub fn get_balance_key(address: Address) -> rU256 {
    storage_key_for_eth_balance(&address.to_h160()).key().to_ru256()
}

/// Represents additional data for ZK transactions.
#[derive(Clone, Debug, Default)]
pub struct ZkTransactionMetadata {
    /// Factory Deps for ZK transactions.
    pub factory_deps: Vec<Vec<u8>>,
}

pub fn new_tx<M: Middleware>(
    legacy_or_1559: TypedTransaction,
    factory_deps: Vec<Vec<u8>>,
    provider: M,
) -> Bytes {
    futures::executor::block_on(new_tx_fut(legacy_or_1559, factory_deps, provider))
}

pub async fn new_tx_fut<M: Middleware>(
    legacy_or_1559: TypedTransaction,
    factory_deps: Vec<Vec<u8>>,
    provider: M,
) -> Bytes {
    let custom_data = Eip712Meta::new().factory_deps(factory_deps);

    let mut deploy_request = Eip712TransactionRequest::new()
        .r#type(EIP712_TX_TYPE)
        .to(*legacy_or_1559.to().and_then(|to| to.as_address()).unwrap())
        .chain_id(legacy_or_1559.chain_id().unwrap().as_u64())
        .nonce(legacy_or_1559.nonce().unwrap())
        .gas_price(legacy_or_1559.gas_price().unwrap())
        .max_fee_per_gas(legacy_or_1559.max_cost().unwrap())
        .data(legacy_or_1559.data().cloned().unwrap())
        .custom_data(custom_data);
    if let Some(from) = legacy_or_1559.from() {
        deploy_request = deploy_request.from(*from)
    }

    let gas_price = provider.get_gas_price().await.unwrap();
    let fee: zksync_web3_rs::zks_provider::types::Fee =
        provider.provider().request("zks_estimateFee", [deploy_request.clone()]).await.unwrap();
    deploy_request = deploy_request
        .gas_limit(fee.gas_limit)
        .max_fee_per_gas(fee.max_fee_per_gas)
        .max_priority_fee_per_gas(fee.max_priority_fee_per_gas)
        .gas_price(gas_price);

    // let signable: Eip712Transaction =
    //     deploy_request.clone().try_into().expect("converting deploy request");

    // let signature =
    //     signer.sign_typed_data(&signable).await.wrap_err("Failed to sign typed data")?;

    let encoded_rlp = deploy_request.rlp_unsigned().unwrap().to_vec();
    [&[EIP712_TX_TYPE], encoded_rlp.as_slice()].concat().into()
}
