use std::fmt::Debug;

use alloy_primitives::{Bytes, B256};
use revm::{
    primitives::{Address, Bytecode, Env, U256 as rU256},
    Database, JournaledState,
};
use tracing::info;
use zksync_types::{
    block::{pack_block_info, unpack_block_info},
    get_nonce_key,
    utils::storage_key_for_eth_balance,
    ACCOUNT_CODE_STORAGE_ADDRESS, CURRENT_VIRTUAL_BLOCK_INFO_POSITION, KNOWN_CODES_STORAGE_ADDRESS,
    L2_ETH_TOKEN_ADDRESS, NONCE_HOLDER_ADDRESS, SYSTEM_CONTEXT_ADDRESS,
};
use zksync_utils::bytecode::hash_bytecode;

use crate::{
    convert::{ConvertAddress, ConvertH160, ConvertH256, ConvertRU256, ConvertU256},
    EMPTY_CODE,
};

/// Sets `block.timestamp`.
pub fn warp<'a, DB>(
    timestamp: rU256,
    env: &'a mut Env,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?timestamp, "cheatcode warp");

    let system_account = SYSTEM_CONTEXT_ADDRESS.to_address();
    journaled_state.load_account(system_account, db).expect("account could not be loaded");
    let block_info_key = CURRENT_VIRTUAL_BLOCK_INFO_POSITION.to_ru256();
    let (block_info, _) =
        journaled_state.sload(system_account, block_info_key, db).unwrap_or_default();
    let (block_number, _block_timestamp) = unpack_block_info(block_info.to_u256());
    let new_block_info = pack_block_info(block_number, timestamp.as_limbs()[0]).to_ru256();

    journaled_state.touch(&system_account);
    journaled_state
        .sstore(system_account, block_info_key, new_block_info, db)
        .expect("failed storing value");
    env.block.timestamp = timestamp;
}

/// Sets `block.number`.
pub fn roll<'a, DB>(
    number: rU256,
    env: &'a mut Env,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?number, "cheatcode roll");

    let system_account = SYSTEM_CONTEXT_ADDRESS.to_address();
    journaled_state.load_account(system_account, db).expect("account could not be loaded");
    let block_info_key = CURRENT_VIRTUAL_BLOCK_INFO_POSITION.to_ru256();
    let (block_info, _) =
        journaled_state.sload(system_account, block_info_key, db).unwrap_or_default();
    let (_block_number, block_timestamp) = unpack_block_info(block_info.to_u256());
    let new_block_info = pack_block_info(number.as_limbs()[0], block_timestamp).to_ru256();

    journaled_state.touch(&system_account);
    journaled_state
        .sstore(system_account, block_info_key, new_block_info, db)
        .expect("failed storing value");
    env.block.number = number;
}

/// Sets balance for a specific address.
pub fn deal<'a, DB>(
    address: Address,
    balance: rU256,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) -> rU256
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?address, ?balance, "cheatcode deal");

    let balance_addr = L2_ETH_TOKEN_ADDRESS.to_address();
    journaled_state.load_account(balance_addr, db).expect("account could not be loaded");
    let zk_address = address.to_h160();
    let balance_key = storage_key_for_eth_balance(&zk_address).key().to_ru256();
    let (old_balance, _) = journaled_state.sload(balance_addr, balance_key, db).unwrap_or_default();
    journaled_state.touch(&balance_addr);
    journaled_state.sstore(balance_addr, balance_key, balance, db).expect("failed storing value");

    old_balance
}

/// Sets nonce for a specific address.
pub fn set_nonce<'a, DB>(
    address: Address,
    nonce: rU256,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?address, ?nonce, "cheatcode setNonce");

    let nonce_addr = NONCE_HOLDER_ADDRESS.to_address();
    journaled_state.load_account(nonce_addr, db).expect("account could not be loaded");
    let zk_address = address.to_h160();
    let nonce_key = get_nonce_key(&zk_address).key().to_ru256();
    journaled_state.touch(&nonce_addr);
    journaled_state.sstore(nonce_addr, nonce_key, nonce, db).expect("failed storing value");
}

/// Gets nonce for a specific address.
pub fn get_nonce<'a, DB>(
    address: Address,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) -> rU256
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?address, "cheatcode getNonce");

    let nonce_addr = NONCE_HOLDER_ADDRESS.to_address();
    journaled_state.load_account(nonce_addr, db).expect("account could not be loaded");
    let zk_address = address.to_h160();
    let nonce_key = get_nonce_key(&zk_address).key().to_ru256();
    let (nonce, _) = journaled_state.sload(nonce_addr, nonce_key, db).unwrap_or_default();

    nonce
}

/// Sets code for a specific address.
pub fn etch<'a, DB>(
    address: Address,
    bytecode: &[u8],
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?address, bytecode = hex::encode(bytecode), "cheatcode etch");

    let bytecode_hash = hash_bytecode(bytecode).to_ru256();
    let bytecode = Bytecode::new_raw(Bytes::copy_from_slice(bytecode));

    let account_code_addr = ACCOUNT_CODE_STORAGE_ADDRESS.to_address();
    let known_codes_addr = KNOWN_CODES_STORAGE_ADDRESS.to_address();
    journaled_state.load_account(account_code_addr, db).expect("account could not be loaded");
    journaled_state.touch(&account_code_addr);
    journaled_state.load_account(known_codes_addr, db).expect("account could not be loaded");
    journaled_state.touch(&known_codes_addr);

    let zk_address = address.to_h160();

    journaled_state
        .sstore(account_code_addr, zk_address.to_h256().to_ru256(), bytecode_hash, db)
        .expect("failed storing value");
    journaled_state
        .sstore(known_codes_addr, bytecode_hash, rU256::ZERO, db)
        .expect("failed storing value");

    journaled_state.load_account(address, db).expect("account could not be loaded");
    journaled_state.touch(&address);
    let account = journaled_state.state.get_mut(&address).expect("failed loading account");
    account.info.code_hash = B256::from(bytecode_hash.to_be_bytes());
    account.info.code = Some(bytecode.clone());
}

/// Sets code for a mocked account. If not done, the mocked call will revert.
/// The call has no effect if the mocked account already has a bytecode entry.
pub fn set_mocked_account<'a, DB>(
    address: Address,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
    caller: Address,
) where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    if address == caller {
        tracing::error!("using `mockCall` cheatcode on caller isn't supported in zkVM");
    }

    let account_code_addr = zksync_types::ACCOUNT_CODE_STORAGE_ADDRESS.to_address();
    let known_code_addr = zksync_types::KNOWN_CODES_STORAGE_ADDRESS.to_address();
    {
        journaled_state
            .load_account(account_code_addr, db)
            .expect("account 'ACCOUNT_CODE_STORAGE_ADDRESS' could not be loaded");
        journaled_state
            .load_account(known_code_addr, db)
            .expect("account 'KNOWN_CODES_STORAGE_ADDRESS' could not be loaded");
    }

    let empty_code_hash = zksync_utils::bytecode::hash_bytecode(&EMPTY_CODE);

    // update account code storage for empty code
    let account_key = address.to_h256().to_ru256();
    let has_code = journaled_state
        .sload(account_code_addr, account_key, db)
        .map(|(v, _)| !v.is_zero())
        .unwrap_or_default();
    if has_code {
        return;
    }

    // update known code storage for empty code
    journaled_state.touch(&account_code_addr);
    journaled_state
        .sstore(account_code_addr, account_key, empty_code_hash.to_ru256(), db)
        .expect("failed storing value");

    let hash_key = empty_code_hash.to_ru256();
    let has_hash = journaled_state
        .sload(known_code_addr, hash_key, db)
        .map(|(v, _)| !v.is_zero())
        .unwrap_or_default();
    if !has_hash {
        journaled_state.touch(&known_code_addr);
        journaled_state
            .sstore(known_code_addr, hash_key, rU256::from(1u32), db)
            .expect("failed storing value");
    }
}
