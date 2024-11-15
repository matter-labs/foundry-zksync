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
use alloy_primitives::{address, hex, keccak256, Address, Bytes, U256 as rU256};
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
use serde::{Deserialize, Serialize};

pub use utils::{fix_l2_gas_limit, fix_l2_gas_price};
pub use vm::{balance, encode_create_params, nonce};

pub use vm::{SELECTOR_CONTRACT_DEPLOYER_CREATE, SELECTOR_CONTRACT_DEPLOYER_CREATE2};
pub use zksync_multivm::interface::{Call, CallType};
pub use zksync_types::{
    ethabi, ACCOUNT_CODE_STORAGE_ADDRESS, CONTRACT_DEPLOYER_ADDRESS, H256,
    IMMUTABLE_SIMULATOR_STORAGE_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS, L2_BASE_TOKEN_ADDRESS,
    NONCE_HOLDER_ADDRESS,
};
use zksync_types::{utils::storage_key_for_eth_balance, U256};
pub use zksync_utils::bytecode::hash_bytecode;
pub use zksync_web3_rs::{
    eip712::{Eip712Meta, Eip712Transaction, Eip712TransactionRequest, PaymasterParams},
    zks_provider::types::Fee,
    zks_utils::EIP712_TX_TYPE,
};

type Result<T> = std::result::Result<T, eyre::Report>;

/// Represents an empty code
pub const EMPTY_CODE: [u8; 32] = [0; 32];

/// The minimum possible address that is not reserved in the zkSync space.
const MIN_VALID_ADDRESS: u32 = 2u32.pow(16);

/// The default CREATE2 deployer for zkSync (0x0000000000000000000000000000000000010000)
/// See: https://github.com/zkSync-Community-Hub/zksync-developers/discussions/519
pub const DEFAULT_CREATE2_DEPLOYER_ZKSYNC: Address =
    address!("0000000000000000000000000000000000010000");

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

/// Represents additional data for ZK transactions that require a paymaster.
#[derive(Clone, Debug, Default)]
pub struct ZkPaymasterData {
    /// Paymaster address.
    pub address: Address,
    /// Paymaster input.
    pub input: Bytes,
}

/// Represents additional data for ZK transactions.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ZkTransactionMetadata {
    /// Factory Deps for ZK transactions.
    pub factory_deps: Vec<Vec<u8>>,
    /// Paymaster data for ZK transactions.
    pub paymaster_data: Option<PaymasterParams>,
}

impl ZkTransactionMetadata {
    /// Create a new [`ZkTransactionMetadata`] with the given factory deps
    pub fn new(factory_deps: Vec<Vec<u8>>, paymaster_data: Option<PaymasterParams>) -> Self {
        Self { factory_deps, paymaster_data }
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
    paymaster_data: Option<PaymasterParams>,
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
    let mut custom_data = Eip712Meta::new().factory_deps(factory_deps);
    if let Some(params) = paymaster_data {
        custom_data = custom_data.paymaster_params(params);
    }

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
    pub limit: u64,
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

    Ok(EstimatedGas { price: gas_price, limit: fee.gas_limit.low_u64() })
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

/// https://github.com/matter-labs/era-contracts/blob/main/system-contracts/contracts/ContractDeployer.sol#L148
const SIGNATURE_CREATE: &str = "create(bytes32,bytes32,bytes)";
/// https://github.com/matter-labs/era-contracts/blob/main/system-contracts/contracts/ContractDeployer.sol#L133
const SIGNATURE_CREATE2: &str = "create2(bytes32,bytes32,bytes)";

/// Try decoding the provided transaction data into create2 parameters.
pub fn try_decode_create2(data: &[u8]) -> Result<(H256, H256, Vec<u8>)> {
    let decoded_calldata =
        foundry_common::abi::abi_decode_calldata(SIGNATURE_CREATE2, &hex::encode(data), true, true)
            .map_err(|err| eyre!("failed decoding data: {err:?}"))?;

    if decoded_calldata.len() < 3 {
        eyre::bail!(
            "failed decoding data, invalid length of {} instead of 3",
            decoded_calldata.len()
        );
    }
    let (salt, bytecode_hash, constructor_args) =
        (&decoded_calldata[0], &decoded_calldata[1], &decoded_calldata[2]);

    let Some(salt) = salt.as_word() else {
        eyre::bail!("failed decoding salt {salt:?}");
    };
    let Some(bytecode_hash) = bytecode_hash.as_word() else {
        eyre::bail!("failed decoding bytecode hash {bytecode_hash:?}");
    };
    let Some(constructor_args) = constructor_args.as_bytes() else {
        eyre::bail!("failed decoding constructor args {constructor_args:?}");
    };

    Ok((H256(salt.0), H256(bytecode_hash.0), constructor_args.to_vec()))
}

/// Try decoding the provided transaction data into create parameters.
pub fn try_decode_create(data: &[u8]) -> Result<(H256, Vec<u8>)> {
    let decoded_calldata =
        foundry_common::abi::abi_decode_calldata(SIGNATURE_CREATE, &hex::encode(data), true, true)
            .map_err(|err| eyre!("failed decoding data: {err:?}"))?;

    if decoded_calldata.len() < 2 {
        eyre::bail!(
            "failed decoding data, invalid length of {} instead of 2",
            decoded_calldata.len()
        );
    }
    let (_salt, bytecode_hash, constructor_args) =
        (&decoded_calldata[0], &decoded_calldata[1], &decoded_calldata[2]);

    let Some(bytecode_hash) = bytecode_hash.as_word() else {
        eyre::bail!("failed decoding bytecode hash {bytecode_hash:?}");
    };
    let Some(constructor_args) = constructor_args.as_bytes() else {
        eyre::bail!("failed decoding constructor args {constructor_args:?}");
    };

    Ok((H256(bytecode_hash.0), constructor_args.to_vec()))
}

/// Gets the mapping key for the `ImmutableSimulator::immutableDataStorage`.
///
/// This retrieves the key for a given contract address and variable slot.
/// See https://github.com/matter-labs/era-contracts/blob/main/system-contracts/contracts/ImmutableSimulator.sol#L21
pub fn get_immutable_slot_key(address: Address, slot_index: rU256) -> H256 {
    let immutable_data_storage_key = keccak256(ethabi::encode(&[
        ethabi::Token::Address(address.to_h160()),
        ethabi::Token::Uint(U256::zero()),
    ]));
    let immutable_data_storage_key = H256(*immutable_data_storage_key);

    let immutable_value_key = keccak256(ethabi::encode(&[
        ethabi::Token::Uint(slot_index.to_u256()),
        ethabi::Token::FixedBytes(immutable_data_storage_key.to_fixed_bytes().to_vec()),
    ]));

    H256(*immutable_value_key)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_get_immutable_slot_key() {
        let actual_key = get_immutable_slot_key(
            address!("f9e9ba9ed9b96ab918c74b21dd0f1d5f2ac38a30"),
            rU256::from(10u32),
        );
        let expected_key =
            H256::from_str("db259b642223206a098c9ffaaf8e4bfd2d60060e8365bb349b2ea2b720d9837c")
                .expect("invalid h256");
        assert_eq!(expected_key, actual_key)
    }
}
