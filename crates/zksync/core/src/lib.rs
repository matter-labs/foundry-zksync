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

use alloy_network::{AnyNetwork, TxSigner};
use alloy_primitives::{Address, Bytes, U256 as rU256};
use alloy_provider::Provider;
use alloy_rpc_types::TransactionRequest;
use alloy_serde::WithOtherFields;
use alloy_signer::Signature;
use alloy_transport::Transport;
use convert::{
    ConvertAddress, ConvertBytes, ConvertH160, ConvertH256, ConvertRU256, ConvertSignature,
    ToSignable,
};
use eyre::{eyre, OptionExt};
pub use utils::{fix_l2_gas_limit, fix_l2_gas_price};
pub use vm::{balance, encode_create_params, nonce};

use zksync_types::utils::storage_key_for_eth_balance;
pub use zksync_types::{
    ACCOUNT_CODE_STORAGE_ADDRESS, CONTRACT_DEPLOYER_ADDRESS, H256, L2_BASE_TOKEN_ADDRESS,
    NONCE_HOLDER_ADDRESS,
};
pub use zksync_utils::bytecode::hash_bytecode;
use zksync_web3_rs::{
    eip712::{Eip712Meta, Eip712Transaction, Eip712TransactionRequest},
    zks_provider::types::Fee,
    zks_utils::EIP712_TX_TYPE,
};

type Result<T> = std::result::Result<T, eyre::Report>;

/// Represents an empty code
pub const EMPTY_CODE: [u8; 32] = [0; 32];

/// The minimum possible address that is not reserved in the zkSync space.
const MIN_VALID_ADDRESS: u32 = 2u32.pow(16);

/// Returns the balance key for a provided account address.
pub fn get_balance_key(address: Address) -> rU256 {
    storage_key_for_eth_balance(&address.to_h160()).key().to_ru256()
}

/// Returns the account code storage key for a provided account address.
pub fn get_account_code_key(address: Address) -> rU256 {
    zksync_types::get_code_key(&address.to_h160()).key().to_ru256()
}

/// Returns the account nonce key for a provided account address.
pub fn get_nonce_key(address: Address) -> rU256 {
    zksync_types::get_nonce_key(&address.to_h160()).key().to_ru256()
}

/// Represents additional data for ZK transactions.
#[derive(Clone, Debug, Default)]
pub struct ZkTransactionMetadata {
    /// Factory Deps for ZK transactions.
    pub factory_deps: Vec<Vec<u8>>,
}

impl ZkTransactionMetadata {
    /// Create a new [`ZkTransactionMetadata`] with the given factory deps
    pub fn new(factory_deps: Vec<Vec<u8>>) -> Self {
        Self { factory_deps }
    }
}

/// Creates a new signed EIP-712 transaction with the provided factory deps.
pub async fn new_eip712_transaction<
    P: Provider<T, AnyNetwork>,
    S: TxSigner<Signature> + Sync,
    T: Transport + Clone,
>(
    tx: WithOtherFields<TransactionRequest>,
    factory_deps: Vec<Vec<u8>>,
    provider: P,
    signer: S,
) -> Result<Bytes> {
    let from = tx.from.ok_or_eyre("`from` cannot be empty")?;
    let to = tx
        .to
        .and_then(|to| match to {
            alloy_primitives::TxKind::Create => None,
            alloy_primitives::TxKind::Call(to) => Some(to),
        })
        .ok_or_eyre("`to` cannot be empty")?;
    let chain_id = tx.chain_id.ok_or_eyre("`chain_id` cannot be empty")?;
    let nonce = tx.nonce.ok_or_eyre("`nonce` cannot be empty")?;
    let gas_price = tx.gas_price.ok_or_eyre("`gas_price` cannot be empty")?;

    let data = tx.input.clone().into_input().unwrap_or_default();
    let custom_data = Eip712Meta::new().factory_deps(factory_deps);

    let mut deploy_request = Eip712TransactionRequest::new()
        .r#type(EIP712_TX_TYPE)
        .from(from.to_h160())
        .to(to.to_h160())
        .chain_id(chain_id)
        .nonce(nonce)
        .gas_price(gas_price)
        .data(data.to_ethers())
        .custom_data(custom_data);

    let gas_price = provider
        .get_gas_price()
        .await
        .map_err(|err| eyre!("failed retrieving gas_price {:?}", err))?;
    let fee: Fee = provider
        .raw_request("zks_estimateFee".into(), [deploy_request.clone()])
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

    let mut signable = signable.to_signable_tx();
    let signature =
        signer.sign_transaction(&mut signable).await.expect("Failed to sign typed data");
    let encoded_rlp = deploy_request
        .rlp_signed(signature.to_ethers())
        .map_err(|err| eyre!("failed encoding deployment request {:?}", err))?;

    let tx = [&[EIP712_TX_TYPE], encoded_rlp.to_vec().as_slice()].concat().into();

    Ok(tx)
}

/// Estimated gas from a ZK network.
pub struct EstimatedGas {
    /// Estimated gas price.
    pub price: u128,
    /// Estimated gas limit.
    pub limit: u128,
}

/// Estimates the gas parameters for the provided transaction.
pub async fn estimate_gas<P: Provider<T, AnyNetwork>, T: Transport + Clone>(
    tx: &WithOtherFields<TransactionRequest>,
    factory_deps: Vec<Vec<u8>>,
    provider: P,
) -> Result<EstimatedGas> {
    let to = tx
        .to
        .and_then(|to| match to {
            alloy_primitives::TxKind::Create => None,
            alloy_primitives::TxKind::Call(to) => Some(to),
        })
        .ok_or_eyre("`to` cannot be empty")?;
    let chain_id = tx.chain_id.ok_or_eyre("`chain_id` cannot be empty")?;
    let nonce = tx.nonce.ok_or_eyre("`nonce` cannot be empty")?;
    let gas_price = if let Some(gas_price) = tx.gas_price {
        gas_price
    } else {
        provider.get_gas_price().await?
    };
    let data = tx.input.clone().into_input().unwrap_or_default();
    let custom_data = Eip712Meta::new().factory_deps(factory_deps);

    let mut deploy_request = Eip712TransactionRequest::new()
        .r#type(EIP712_TX_TYPE)
        .to(to.to_h160())
        .chain_id(chain_id)
        .nonce(nonce)
        .gas_price(gas_price)
        .data(data.to_ethers())
        .custom_data(custom_data);
    if let Some(from) = tx.from {
        deploy_request = deploy_request.from(from.to_h160())
    }

    let gas_price = provider.get_gas_price().await.unwrap();
    let fee: Fee = provider
        .raw_request("zks_estimateFee".into(), [deploy_request.clone()])
        .await
        .map_err(|err| eyre!("failed rpc call for estimating fee: {:?}", err))?;

    Ok(EstimatedGas { price: gas_price, limit: fee.gas_limit.low_u128() })
}

/// Returns true if the provided address is a reserved zkSync system address
/// All addresses less than 2^16 are considered reserved addresses.
pub fn is_system_address(address: Address) -> bool {
    address.to_h256().to_ru256().lt(&rU256::from(MIN_VALID_ADDRESS))
}

/// Creates a safe address from the input address, by offsetting a reserved address
/// byt [MIN_VALID_ADDRESS] so it is above the system reserved address space of 2^16.
pub fn to_safe_address(address: Address) -> Address {
    if is_system_address(address) {
        address
            .to_ru256()
            .saturating_add(rU256::from(MIN_VALID_ADDRESS))
            .to_h256()
            .to_h160()
            .to_address()
    } else {
        address
    }
}
