use alloy_primitives::{Bytes, B256};
use revm::{
    primitives::{
        Account, AccountInfo, Address as rAddress, Bytecode, Env, HashMap as rHashMap, StorageSlot,
        KECCAK_EMPTY, U256 as rU256,
    },
    Database, JournaledState,
};
use std::fmt::Debug;
use tracing::info;
use zksync_types::{
    block::pack_block_info,
    get_is_account_key, get_nonce_key,
    utils::{decompose_full_nonce, storage_key_for_eth_balance},
    ACCOUNT_CODE_STORAGE_ADDRESS, CURRENT_VIRTUAL_BLOCK_INFO_POSITION, KNOWN_CODES_STORAGE_ADDRESS,
    L2_ETH_TOKEN_ADDRESS, NONCE_HOLDER_ADDRESS, SYSTEM_CONTEXT_ADDRESS,
};

use crate::{
    convert::{ConvertAddress, ConvertH160, ConvertH256, ConvertRU256, ConvertU256},
    DualCompiledContract,
};

/// Returns balance storage slot
pub fn mark_account_eoa<'a, DB>(
    address: rAddress,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    let is_account_key = get_is_account_key(&address.to_h160());
    if let Ok((value, _)) = journaled_state.sload(
        is_account_key.address().to_address(),
        is_account_key.key().to_ru256(),
        db,
    ) {
        println!("IS ACCOUNT {:?}", value);
    }
}

/// Returns balance storage slot
pub fn get_balance_storage(address: rAddress) -> (rAddress, rU256) {
    let balance_key = storage_key_for_eth_balance(&address.to_h160());
    let account = balance_key.address().to_address();
    let slot = balance_key.key().to_ru256();
    (account, slot)
}

/// Returns nonce storage slot
pub fn get_nonce_storage(address: rAddress) -> (rAddress, rU256) {
    let nonce_key = get_nonce_key(&address.to_h160());
    let account = nonce_key.address().to_address();
    let slot = nonce_key.key().to_ru256();
    (account, slot)
}

/// Synchronizes the provided accounts in ZK state
pub fn sync_accounts_to_evm<'a, DB>(
    accounts: Vec<rAddress>,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    let balance_addr = L2_ETH_TOKEN_ADDRESS.to_address();
    let nonce_addr = NONCE_HOLDER_ADDRESS.to_address();
    for address in accounts {
        info!(?address, "sync to evm state");

        let account = journaled_account(db, journaled_state, address);
        let info = &account.info;
        let zk_address = address.to_h160();

        println!("balance = {:?}", info.balance);
        let balance_key = storage_key_for_eth_balance(&zk_address).key().to_ru256();
        let nonce_key = get_nonce_key(&zk_address).key().to_ru256();

        let (balance, _) = journaled_state.sload(balance_addr, balance_key, db).unwrap_or_default();
        let (full_nonce, _) = journaled_state.sload(nonce_addr, nonce_key, db).unwrap_or_default();
        let (tx_nonce, _deployment_nonce) = decompose_full_nonce(full_nonce.to_u256());
        let nonce = tx_nonce.as_u64();

        let account = journaled_account(db, journaled_state, address);
        let _ = std::mem::replace(&mut account.info.balance, balance);
        let _ = std::mem::replace(&mut account.info.nonce, nonce);
    }
}

/// Synchronizes the provided accounts in ZK state
pub fn sync_accounts<'a, DB>(
    accounts: Vec<rAddress>,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    let mut l2_eth_storage: rHashMap<rU256, StorageSlot> = Default::default();
    let mut nonce_storage: rHashMap<rU256, StorageSlot> = Default::default();

    for address in accounts {
        info!(?address, "sync to zk state");

        let account = journaled_account(db, journaled_state, address);
        let info = &account.info;
        let zk_address = address.to_h160();

        println!("balance = {:?}", info.balance);
        let balance_key = storage_key_for_eth_balance(&zk_address).key().to_ru256();
        let nonce_key = get_nonce_key(&zk_address).key().to_ru256();
        l2_eth_storage.insert(balance_key, StorageSlot::new(info.balance));
        nonce_storage.insert(nonce_key, StorageSlot::new(rU256::from(info.nonce)));
    }

    let balance_addr = L2_ETH_TOKEN_ADDRESS.to_address();
    let balance_account = journaled_account(db, journaled_state, balance_addr);
    balance_account.storage.extend(l2_eth_storage.clone());

    let nonce_addr = NONCE_HOLDER_ADDRESS.to_address();
    let nonce_account = journaled_account(db, journaled_state, nonce_addr);
    nonce_account.storage.extend(nonce_storage.clone());
}

/// Synchronizes the provided accounts in ZK state
pub fn migrate_to_zk<'a, DB>(
    accounts: Vec<rAddress>,
    dual_compiled_contracts: &[DualCompiledContract],
    env: &Env,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    let mut system_storage: rHashMap<rU256, StorageSlot> = Default::default();
    let block_info_key = CURRENT_VIRTUAL_BLOCK_INFO_POSITION.to_ru256();
    let block_info =
        pack_block_info(env.block.number.as_limbs()[0], env.block.timestamp.as_limbs()[0]);
    system_storage.insert(block_info_key, StorageSlot::new(block_info.to_ru256()));

    let mut l2_eth_storage: rHashMap<rU256, StorageSlot> = Default::default();
    let mut nonce_storage: rHashMap<rU256, StorageSlot> = Default::default();
    let mut account_code_storage: rHashMap<rU256, StorageSlot> = Default::default();
    let mut known_codes_storage: rHashMap<rU256, StorageSlot> = Default::default();
    let mut deployed_codes: rHashMap<rAddress, AccountInfo> = Default::default();

    for address in accounts {
        info!(?address, "importing to zk state");

        let account = journaled_account(db, journaled_state, address);
        let info = &account.info;
        let zk_address = address.to_h160();

        println!("balance = {:?}", info.balance);
        let balance_key = storage_key_for_eth_balance(&zk_address).key().to_ru256();
        let nonce_key = get_nonce_key(&zk_address).key().to_ru256();
        l2_eth_storage.insert(balance_key, StorageSlot::new(info.balance));
        nonce_storage.insert(nonce_key, StorageSlot::new(rU256::from(info.nonce)));

        if let Some(contract) = dual_compiled_contracts.iter().find(|contract| {
            info.code_hash != KECCAK_EMPTY && info.code_hash == contract.evm_bytecode_hash
        }) {
            account_code_storage.insert(
                zk_address.to_h256().to_ru256(),
                StorageSlot::new(contract.zk_bytecode_hash.to_ru256()),
            );
            known_codes_storage
                .insert(contract.zk_bytecode_hash.to_ru256(), StorageSlot::new(rU256::ZERO));

            let code_hash = B256::from_slice(contract.zk_bytecode_hash.as_bytes());
            deployed_codes.insert(
                address,
                AccountInfo {
                    balance: info.balance,
                    nonce: info.nonce,
                    code_hash,
                    code: Some(Bytecode::new_raw(Bytes::from(
                        contract.zk_deployed_bytecode.clone(),
                    ))),
                },
            );
        }
    }

    let system_addr = SYSTEM_CONTEXT_ADDRESS.to_address();
    let system_account = journaled_account(db, journaled_state, system_addr);
    system_account.storage.extend(system_storage.clone());

    let balance_addr = L2_ETH_TOKEN_ADDRESS.to_address();
    let balance_account = journaled_account(db, journaled_state, balance_addr);
    balance_account.storage.extend(l2_eth_storage.clone());

    let nonce_addr = NONCE_HOLDER_ADDRESS.to_address();
    let nonce_account = journaled_account(db, journaled_state, nonce_addr);
    nonce_account.storage.extend(nonce_storage.clone());

    let account_code_addr = ACCOUNT_CODE_STORAGE_ADDRESS.to_address();
    let account_code_account = journaled_account(db, journaled_state, account_code_addr);
    account_code_account.storage.extend(account_code_storage.clone());

    let known_codes_addr = KNOWN_CODES_STORAGE_ADDRESS.to_address();
    let known_codes_account = journaled_account(db, journaled_state, known_codes_addr);
    known_codes_account.storage.extend(known_codes_storage.clone());

    for (address, info) in deployed_codes {
        let account = journaled_account(db, journaled_state, address);
        let _ = std::mem::replace(&mut account.info.balance, info.balance);
        let _ = std::mem::replace(&mut account.info.nonce, info.nonce);
        account.info.code_hash = info.code_hash;
        account.info.code = info.code.clone();
    }
}

fn journaled_account<'a, DB>(
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
    addr: rAddress,
) -> &'a mut Account
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    journaled_state.load_account(addr, db).expect("failed loading account");
    journaled_state.touch(&addr);
    journaled_state.state.get_mut(&addr).expect("account is loaded")
}
