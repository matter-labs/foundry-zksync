use std::fmt::Debug;

use alloy_primitives::{Bytes, B256};
use foundry_common::{
    conversion_utils::{address_to_h160, u256_to_revm_u256},
    zk_utils::conversion_utils::{h160_to_address, h256_to_revm_u256, revm_u256_to_u256},
};
use revm::{
    primitives::{Address, Bytecode, Env, U256 as rU256},
    Database, JournaledState,
};
use zksync_types::{
    block::{pack_block_info, unpack_block_info},
    get_nonce_key,
    utils::storage_key_for_eth_balance,
    ACCOUNT_CODE_STORAGE_ADDRESS, CURRENT_VIRTUAL_BLOCK_INFO_POSITION, KNOWN_CODES_STORAGE_ADDRESS,
    L2_ETH_TOKEN_ADDRESS, NONCE_HOLDER_ADDRESS, SYSTEM_CONTEXT_ADDRESS,
};
use zksync_utils::{address_to_h256, bytecode::hash_bytecode};

pub(crate) fn warp<'a, DB>(
    timestamp: rU256,
    env: &'a mut Env,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?timestamp, "cheatcode warp");

    let system_account = h160_to_address(SYSTEM_CONTEXT_ADDRESS);
    journaled_state.load_account(system_account, db).expect("account could not be loaded");
    let block_info_key = h256_to_revm_u256(CURRENT_VIRTUAL_BLOCK_INFO_POSITION);
    let (block_info, _) =
        journaled_state.sload(system_account, block_info_key, db).unwrap_or_default();
    let (block_number, _block_timestamp) = unpack_block_info(revm_u256_to_u256(block_info));
    let new_block_info = u256_to_revm_u256(pack_block_info(block_number, timestamp.as_limbs()[0]));

    journaled_state.touch(&system_account);
    journaled_state
        .sstore(system_account, block_info_key, new_block_info, db)
        .expect("failed storing value");
    env.block.timestamp = timestamp;
}

pub(crate) fn roll<'a, DB>(
    number: rU256,
    env: &'a mut Env,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?number, "cheatcode roll");

    let system_account = h160_to_address(SYSTEM_CONTEXT_ADDRESS);
    journaled_state.load_account(system_account, db).expect("account could not be loaded");
    let block_info_key = h256_to_revm_u256(CURRENT_VIRTUAL_BLOCK_INFO_POSITION);
    let (block_info, _) =
        journaled_state.sload(system_account, block_info_key, db).unwrap_or_default();
    let (_block_number, block_timestamp) = unpack_block_info(revm_u256_to_u256(block_info));
    let new_block_info = u256_to_revm_u256(pack_block_info(number.as_limbs()[0], block_timestamp));

    journaled_state.touch(&system_account);
    journaled_state
        .sstore(system_account, block_info_key, new_block_info, db)
        .expect("failed storing value");
    env.block.number = number;
}

pub(crate) fn deal<'a, DB>(
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

    let balance_addr = h160_to_address(L2_ETH_TOKEN_ADDRESS);
    journaled_state.load_account(balance_addr, db).expect("account could not be loaded");
    let zk_address = address_to_h160(address);
    let balance_key = h256_to_revm_u256(*storage_key_for_eth_balance(&zk_address).key());
    let (old_balance, _) = journaled_state.sload(balance_addr, balance_key, db).unwrap_or_default();
    journaled_state.touch(&balance_addr);
    journaled_state.sstore(balance_addr, balance_key, balance, db).expect("failed storing value");

    old_balance
}

pub(crate) fn set_nonce<'a, DB>(
    address: Address,
    nonce: rU256,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?address, ?nonce, "cheatcode setNonce");

    let nonce_addr = h160_to_address(NONCE_HOLDER_ADDRESS);
    journaled_state.load_account(nonce_addr, db).expect("account could not be loaded");
    let zk_address = address_to_h160(address);
    let nonce_key = h256_to_revm_u256(*get_nonce_key(&zk_address).key());
    journaled_state.touch(&nonce_addr);
    journaled_state.sstore(nonce_addr, nonce_key, nonce, db).expect("failed storing value");
}

pub(crate) fn get_nonce<'a, DB>(
    address: Address,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) -> rU256
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?address, "cheatcode getNonce");

    let nonce_addr = h160_to_address(NONCE_HOLDER_ADDRESS);
    journaled_state.load_account(nonce_addr, db).expect("account could not be loaded");
    let zk_address = address_to_h160(address);
    let nonce_key = h256_to_revm_u256(*get_nonce_key(&zk_address).key());
    let (nonce, _) = journaled_state.sload(nonce_addr, nonce_key, db).unwrap_or_default();

    nonce
}

pub(crate) fn etch<'a, DB>(
    address: Address,
    bytecode: &[u8],
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?address, bytecode = hex::encode(bytecode), "cheatcode etch");

    let bytecode_hash = h256_to_revm_u256(hash_bytecode(bytecode));
    let bytecode = Bytecode::new_raw(Bytes::copy_from_slice(bytecode)).to_checked();

    let account_code_addr = h160_to_address(ACCOUNT_CODE_STORAGE_ADDRESS);
    let known_codes_addr = h160_to_address(KNOWN_CODES_STORAGE_ADDRESS);
    journaled_state.load_account(account_code_addr, db).expect("account could not be loaded");
    journaled_state.touch(&account_code_addr);
    journaled_state.load_account(known_codes_addr, db).expect("account could not be loaded");
    journaled_state.touch(&known_codes_addr);

    let zk_address = address_to_h160(address);

    journaled_state
        .sstore(
            account_code_addr,
            h256_to_revm_u256(address_to_h256(&zk_address)),
            bytecode_hash,
            db,
        )
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
