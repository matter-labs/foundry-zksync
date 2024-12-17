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

use alloy_network::TransactionBuilder;
use alloy_primitives::{address, hex, keccak256, Address, Bytes, U256 as rU256};
use alloy_transport::Transport;
use alloy_zksync::{
    network::transaction_request::TransactionRequest as ZkTransactionRequest,
    provider::ZksyncProvider,
};
use convert::{ConvertAddress, ConvertH160, ConvertH256, ConvertRU256, ConvertU256};
use eyre::eyre;
use revm::{Database, InnerEvmContext};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

pub use utils::{fix_l2_gas_limit, fix_l2_gas_price};
pub use vm::{balance, encode_create_params, nonce};

pub use vm::{SELECTOR_CONTRACT_DEPLOYER_CREATE, SELECTOR_CONTRACT_DEPLOYER_CREATE2};
pub use zksync_multivm::interface::{Call, CallType};
pub use zksync_types::{
    ethabi, transaction_request::PaymasterParams, ACCOUNT_CODE_STORAGE_ADDRESS,
    CONTRACT_DEPLOYER_ADDRESS, H256, IMMUTABLE_SIMULATOR_STORAGE_ADDRESS,
    KNOWN_CODES_STORAGE_ADDRESS, L2_BASE_TOKEN_ADDRESS, NONCE_HOLDER_ADDRESS,
};
use zksync_types::{
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
    U256,
};
pub use zksync_utils::bytecode::hash_bytecode;

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
/// Estimated gas from a ZK network.
pub struct EstimatedGas {
    /// Estimated gas price.
    pub price: u128,
    /// Estimated gas limit.
    pub limit: u64,
}

/// Estimates the gas parameters for the provided transaction.
/// This will call `estimateFee` method on the rpc and set the gas parameters on the transaction.
pub async fn estimate_gas<P: ZksyncProvider<T>, T: Transport + Clone>(
    tx: &mut ZkTransactionRequest,
    provider: P,
) -> Result<()> {
    let fee = provider.estimate_fee(tx.clone()).await?;
    tx.set_gas_limit(fee.gas_limit);
    tx.set_max_fee_per_gas(fee.max_fee_per_gas);
    tx.set_max_priority_fee_per_gas(fee.max_priority_fee_per_gas);
    tx.set_gas_per_pubdata(fee.gas_per_pubdata_limit);

    Ok(())
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

/// Sets transaction nonce for a specific address.
pub fn set_tx_nonce<DB>(address: Address, nonce: rU256, ecx: &mut InnerEvmContext<DB>)
where
    DB: Database,
    DB::Error: Debug,
{
    //ensure nonce is _only_ tx nonce
    let (tx_nonce, _deploy_nonce) = decompose_full_nonce(nonce.to_u256());

    let nonce_addr = NONCE_HOLDER_ADDRESS.to_address();
    ecx.load_account(nonce_addr).expect("account could not be loaded");
    let nonce_key = get_nonce_key(address);
    ecx.touch(&nonce_addr);
    // We make sure to keep the old deployment nonce
    let old_deploy_nonce = ecx
        .sload(nonce_addr, nonce_key)
        .map(|v| decompose_full_nonce(v.to_u256()).1)
        .unwrap_or_default();
    let updated_nonce = nonces_to_full_nonce(tx_nonce, old_deploy_nonce);
    ecx.sstore(nonce_addr, nonce_key, updated_nonce.to_ru256()).expect("failed storing value");
}

/// Sets deployment nonce for a specific address.
pub fn set_deployment_nonce<DB>(address: Address, nonce: rU256, ecx: &mut InnerEvmContext<DB>)
where
    DB: Database,
    DB::Error: Debug,
{
    //ensure nonce is _only_ deployment nonce
    let (_tx_nonce, deploy_nonce) = decompose_full_nonce(nonce.to_u256());

    let nonce_addr = NONCE_HOLDER_ADDRESS.to_address();
    ecx.load_account(nonce_addr).expect("account could not be loaded");
    let nonce_key = get_nonce_key(address);
    ecx.touch(&nonce_addr);
    // We make sure to keep the old transaction nonce
    let old_tx_nonce = ecx
        .sload(nonce_addr, nonce_key)
        .map(|v| decompose_full_nonce(v.to_u256()).0)
        .unwrap_or_default();
    let updated_nonce = nonces_to_full_nonce(old_tx_nonce, deploy_nonce);
    ecx.sstore(nonce_addr, nonce_key, updated_nonce.to_ru256()).expect("failed storing value");
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
