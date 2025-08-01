use std::fmt::Debug;

use alloy_evm::eth::EthEvmContext;
use alloy_primitives::{B256, Bytes, hex};
use revm::{
    Database,
    bytecode::Bytecode,
    context::JournalTr,
    primitives::{Address, U256 as rU256},
};
use tracing::info;
use zksync_types::{
    ACCOUNT_CODE_STORAGE_ADDRESS, CURRENT_VIRTUAL_BLOCK_INFO_POSITION, KNOWN_CODES_STORAGE_ADDRESS,
    L2_BASE_TOKEN_ADDRESS, NONCE_HOLDER_ADDRESS, SYSTEM_CONTEXT_ADDRESS,
    block::{pack_block_info, unpack_block_info},
    get_nonce_key,
    utils::{decompose_full_nonce, storage_key_for_eth_balance},
};

use crate::{
    EMPTY_CODE,
    convert::{ConvertAddress, ConvertH160, ConvertH256, ConvertRU256, ConvertU256},
    hash_bytecode,
};

/// Sets `block.timestamp`.
pub fn warp<DB>(timestamp: rU256, ecx: &mut EthEvmContext<DB>)
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?timestamp, "cheatcode warp");

    let system_account = SYSTEM_CONTEXT_ADDRESS.to_address();
    ecx.journaled_state.load_account(system_account).expect("account could not be loaded");
    let block_info_key = CURRENT_VIRTUAL_BLOCK_INFO_POSITION.to_ru256();
    let block_info = ecx.journaled_state.sload(system_account, block_info_key).unwrap_or_default();
    let (block_number, _block_timestamp) = unpack_block_info(block_info.to_u256());
    let new_block_info = pack_block_info(block_number, timestamp.as_limbs()[0]).to_ru256();

    ecx.journaled_state.touch(system_account);
    ecx.journaled_state
        .sstore(system_account, block_info_key, new_block_info)
        .expect("failed storing value");
    ecx.block.timestamp = timestamp.try_into().expect("Timestamp exceeds u64 in warp cheatcode");
}

/// Sets `block.number`.
pub fn roll<DB>(number: rU256, ecx: &mut EthEvmContext<DB>)
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?number, "cheatcode roll");

    let system_account = SYSTEM_CONTEXT_ADDRESS.to_address();
    ecx.journaled_state.load_account(system_account).expect("account could not be loaded");
    let block_info_key = CURRENT_VIRTUAL_BLOCK_INFO_POSITION.to_ru256();
    let block_info = ecx.journaled_state.sload(system_account, block_info_key).unwrap_or_default();
    let (_block_number, block_timestamp) = unpack_block_info(block_info.to_u256());
    let new_block_info = pack_block_info(number.as_limbs()[0], block_timestamp).to_ru256();

    ecx.journaled_state.touch(system_account);
    ecx.journaled_state
        .sstore(system_account, block_info_key, new_block_info)
        .expect("failed storing value");
    ecx.block.number = number.try_into().expect("Block number exceeds u64 in roll cheatcode");
}

/// Sets balance for a specific address.
pub fn deal<DB>(address: Address, balance: rU256, ecx: &mut EthEvmContext<DB>) -> rU256
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?address, ?balance, "cheatcode deal");

    let balance_addr = L2_BASE_TOKEN_ADDRESS.to_address();
    ecx.journaled_state.load_account(balance_addr).expect("account could not be loaded");
    let zk_address = address.to_h160();
    let balance_key = storage_key_for_eth_balance(&zk_address).key().to_ru256();
    let old_balance = ecx.journaled_state.sload(balance_addr, balance_key).unwrap_or_default();
    ecx.journaled_state.touch(balance_addr);
    ecx.journaled_state.sstore(balance_addr, balance_key, balance).expect("failed storing value");

    old_balance.data
}

/// Sets nonce for a specific address.
pub fn set_nonce<DB>(address: Address, nonce: rU256, ecx: &mut EthEvmContext<DB>)
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?address, ?nonce, "cheatcode setNonce");
    crate::set_tx_nonce(address, nonce, ecx);
}

/// Gets nonce for a specific address.
pub fn get_nonce<DB>(address: Address, ecx: &mut EthEvmContext<DB>) -> rU256
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?address, "cheatcode getNonce");

    let nonce_addr = NONCE_HOLDER_ADDRESS.to_address();
    ecx.journaled_state.load_account(nonce_addr).expect("account could not be loaded");
    let zk_address = address.to_h160();
    let nonce_key = get_nonce_key(&zk_address).key().to_ru256();
    let full_nonce = ecx.journaled_state.sload(nonce_addr, nonce_key).unwrap_or_default();

    let (tx_nonce, _deploy_nonce) = decompose_full_nonce(full_nonce.to_u256());
    tx_nonce.to_ru256()
}

/// Gets the full nonce for a specific address.
pub fn get_full_nonce<DB>(address: Address, ecx: &mut EthEvmContext<DB>) -> (rU256, rU256)
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    let nonce_addr = NONCE_HOLDER_ADDRESS.to_address();
    ecx.journaled_state.load_account(nonce_addr).expect("account could not be loaded");
    let zk_address = address.to_h160();
    let nonce_key = get_nonce_key(&zk_address).key().to_ru256();
    let full_nonce = ecx.journaled_state.sload(nonce_addr, nonce_key).unwrap_or_default();

    let (tx_nonce, deploy_nonce) = decompose_full_nonce(full_nonce.to_u256());
    (tx_nonce.to_ru256(), deploy_nonce.to_ru256())
}

/// Sets code for a specific address.
pub fn etch<DB>(address: Address, bytecode: &[u8], ecx: &mut EthEvmContext<DB>)
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?address, bytecode = hex::encode(bytecode), "cheatcode etch");
    let len = bytecode.len();
    if len % 32 != 0 {
        panic!(
            "etch bytecode length must be divisible by 32, found '{}' with length {len}",
            hex::encode(bytecode)
        );
    }

    let bytecode_hash = hash_bytecode(bytecode).to_ru256();
    let bytecode = Bytecode::new_raw(Bytes::copy_from_slice(bytecode));

    let account_code_addr = ACCOUNT_CODE_STORAGE_ADDRESS.to_address();
    let known_codes_addr = KNOWN_CODES_STORAGE_ADDRESS.to_address();
    ecx.journaled_state.load_account(account_code_addr).expect("account could not be loaded");
    ecx.journaled_state.touch(account_code_addr);
    ecx.journaled_state.load_account(known_codes_addr).expect("account could not be loaded");
    ecx.journaled_state.touch(known_codes_addr);

    let zk_address = address.to_h160();

    ecx.journaled_state
        .sstore(account_code_addr, zk_address.to_h256().to_ru256(), bytecode_hash)
        .expect("failed storing value");
    ecx.journaled_state
        .sstore(known_codes_addr, bytecode_hash, rU256::ZERO)
        .expect("failed storing value");

    ecx.journaled_state.load_account(address).expect("account could not be loaded");
    ecx.journaled_state.touch(address);
    let account = ecx.journaled_state.state.get_mut(&address).expect("failed loading account");
    account.info.code_hash = B256::from(bytecode_hash.to_be_bytes());
    account.info.code = Some(bytecode.clone());
}

/// Sets code for a mocked account. If not done, the mocked call will revert.
/// The call has no effect if the mocked account already has a bytecode entry.
pub fn set_mocked_account<DB>(address: Address, ecx: &mut EthEvmContext<DB>, caller: Address)
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    if address == caller {
        tracing::error!(
            "using `mockCall` cheatcode on caller ({address:?}) isn't supported in zkVM"
        );
    }

    let account_code_addr = zksync_types::ACCOUNT_CODE_STORAGE_ADDRESS.to_address();
    let known_code_addr = zksync_types::KNOWN_CODES_STORAGE_ADDRESS.to_address();
    {
        ecx.journaled_state
            .load_account(account_code_addr)
            .expect("account 'ACCOUNT_CODE_STORAGE_ADDRESS' could not be loaded");
        ecx.journaled_state
            .load_account(known_code_addr)
            .expect("account 'KNOWN_CODES_STORAGE_ADDRESS' could not be loaded");
    }

    let empty_code_hash = hash_bytecode(&EMPTY_CODE);

    // update account code storage for empty code
    let account_key = address.to_h256().to_ru256();
    let has_code = ecx
        .journaled_state
        .sload(account_code_addr, account_key)
        .map(|v| !v.is_zero())
        .unwrap_or_default();
    if has_code {
        return;
    }

    // update known code storage for empty code
    ecx.journaled_state.touch(account_code_addr);
    ecx.journaled_state
        .sstore(account_code_addr, account_key, empty_code_hash.to_ru256())
        .expect("failed storing value");

    let hash_key = empty_code_hash.to_ru256();
    let has_hash = ecx
        .journaled_state
        .sload(known_code_addr, hash_key)
        .map(|v| !v.is_zero())
        .unwrap_or_default();
    if !has_hash {
        ecx.journaled_state.touch(known_code_addr);
        ecx.journaled_state
            .sstore(known_code_addr, hash_key, rU256::from(1u32))
            .expect("failed storing value");
    }
}

#[cfg(test)]
mod tests {
    use revm::database::EmptyDB;

    use super::*;

    #[test]
    #[should_panic(expected = "bytecode length must be divisible by 32")]
    fn test_etch_panics_when_bytecode_not_aligned_on_32_bytes() {
        etch(Address::ZERO, &[0], &mut EthEvmContext::new(EmptyDB::default(), Default::default()));
    }
}
