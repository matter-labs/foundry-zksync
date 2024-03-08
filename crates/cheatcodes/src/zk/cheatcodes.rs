use std::fmt::Debug;

use foundry_common::{
    conversion_utils::{address_to_h160, u256_to_revm_u256},
    zk_utils::conversion_utils::{h160_to_address, h256_to_revm_u256, revm_u256_to_u256},
};
use revm::{
    primitives::{Address, Env, U256 as rU256},
    Database, JournaledState,
};
use zksync_types::{
    block::{pack_block_info, unpack_block_info},
    utils::storage_key_for_eth_balance,
    CURRENT_VIRTUAL_BLOCK_INFO_POSITION, L2_ETH_TOKEN_ADDRESS, SYSTEM_CONTEXT_ADDRESS,
};

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
