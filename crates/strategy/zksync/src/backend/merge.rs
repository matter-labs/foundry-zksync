use std::collections::hash_map::Entry;

use alloy_primitives::{map::HashMap, Address, U256};

use foundry_evm_core::backend::{strategy::EvmBackendMergeStrategy, Fork, ForkDB};
use foundry_zksync_core::{
    convert::ConvertH160, ACCOUNT_CODE_STORAGE_ADDRESS, IMMUTABLE_SIMULATOR_STORAGE_ADDRESS,
    KNOWN_CODES_STORAGE_ADDRESS, L2_BASE_TOKEN_ADDRESS, NONCE_HOLDER_ADDRESS,
};
use revm::{db::CacheDB, primitives::HashSet, DatabaseRef, JournaledState};
use tracing::trace;

pub(super) struct ZksyncBackendMerge;

/// Defines the zksync specific state to help during merge.
pub(super) struct ZksyncMergeState<'a> {
    pub persistent_immutable_keys: &'a HashMap<Address, HashSet<U256>>,
}

impl ZksyncBackendMerge {
    /// Clones the data of the given `accounts` from the `active` database into the `fork_db`
    /// This includes the data held in storage (`CacheDB`) and kept in the `JournaledState`.
    pub fn merge_account_data<ExtDB: DatabaseRef>(
        accounts: impl IntoIterator<Item = Address>,
        active: &CacheDB<ExtDB>,
        active_journaled_state: &mut JournaledState,
        target_fork: &mut Fork,
        zk_state: &ZksyncMergeState<'_>,
    ) {
        for addr in accounts.into_iter() {
            EvmBackendMergeStrategy::merge_db_account_data(addr, active, &mut target_fork.db);
            Self::merge_zk_account_data(addr, active, &mut target_fork.db, zk_state);
            EvmBackendMergeStrategy::merge_journaled_state_data(
                addr,
                active_journaled_state,
                &mut target_fork.journaled_state,
            );
            Self::merge_zk_journaled_state_data(
                addr,
                active_journaled_state,
                &mut target_fork.journaled_state,
                zk_state,
            );
        }

        // need to mock empty journal entries in case the current checkpoint is higher than the
        // existing journal entries
        while active_journaled_state.journal.len() > target_fork.journaled_state.journal.len() {
            target_fork.journaled_state.journal.push(Default::default());
        }

        *active_journaled_state = target_fork.journaled_state.clone();
    }

    /// Clones the zk account data from the `active` db into the `ForkDB`
    pub(super) fn merge_zk_account_data<ExtDB: DatabaseRef>(
        addr: Address,
        active: &CacheDB<ExtDB>,
        fork_db: &mut ForkDB,
        _zk_state: &ZksyncMergeState<'_>,
    ) {
        let merge_system_contract_entry =
            |fork_db: &mut ForkDB, system_contract: Address, slot: U256| {
                let Some(acc) = active.accounts.get(&system_contract) else { return };

                // port contract cache over
                if let Some(code) = active.contracts.get(&acc.info.code_hash) {
                    trace!("merging contract cache");
                    fork_db.contracts.insert(acc.info.code_hash, code.clone());
                }

                // prepare only the specified slot in account storage
                let mut new_acc = acc.clone();
                new_acc.storage = Default::default();
                if let Some(value) = acc.storage.get(&slot) {
                    new_acc.storage.insert(slot, *value);
                }

                // port account storage over
                match fork_db.accounts.entry(system_contract) {
                    Entry::Vacant(vacant) => {
                        trace!("target account not present - inserting from active");
                        // if the fork_db doesn't have the target account
                        // insert the entire thing
                        vacant.insert(new_acc);
                    }
                    Entry::Occupied(mut occupied) => {
                        trace!("target account present - merging storage slots");
                        // if the fork_db does have the system,
                        // extend the existing storage (overriding)
                        let fork_account = occupied.get_mut();
                        fork_account.storage.extend(&new_acc.storage);
                    }
                }
            };

        merge_system_contract_entry(
            fork_db,
            L2_BASE_TOKEN_ADDRESS.to_address(),
            foundry_zksync_core::get_balance_key(addr),
        );
        merge_system_contract_entry(
            fork_db,
            ACCOUNT_CODE_STORAGE_ADDRESS.to_address(),
            foundry_zksync_core::get_account_code_key(addr),
        );
        merge_system_contract_entry(
            fork_db,
            NONCE_HOLDER_ADDRESS.to_address(),
            foundry_zksync_core::get_nonce_key(addr),
        );

        if let Some(acc) = active.accounts.get(&addr) {
            merge_system_contract_entry(
                fork_db,
                KNOWN_CODES_STORAGE_ADDRESS.to_address(),
                U256::from_be_slice(&acc.info.code_hash.0[..]),
            );
        }
    }

    /// Clones the account data from the `active_journaled_state` into the `fork_journaled_state`
    /// for zksync storage.
    pub(super) fn merge_zk_journaled_state_data(
        addr: Address,
        active_journaled_state: &JournaledState,
        fork_journaled_state: &mut JournaledState,
        zk_state: &ZksyncMergeState<'_>,
    ) {
        let merge_system_contract_entry =
            |fork_journaled_state: &mut JournaledState, system_contract: Address, slot: U256| {
                if let Some(acc) = active_journaled_state.state.get(&system_contract) {
                    // prepare only the specified slot in account storage
                    let mut new_acc = acc.clone();
                    new_acc.storage = Default::default();
                    if let Some(value) = acc.storage.get(&slot).cloned() {
                        new_acc.storage.insert(slot, value);
                    }

                    match fork_journaled_state.state.entry(system_contract) {
                        Entry::Vacant(vacant) => {
                            vacant.insert(new_acc);
                        }
                        Entry::Occupied(mut occupied) => {
                            let fork_account = occupied.get_mut();
                            fork_account.storage.extend(new_acc.storage);
                        }
                    }
                }
            };

        merge_system_contract_entry(
            fork_journaled_state,
            L2_BASE_TOKEN_ADDRESS.to_address(),
            foundry_zksync_core::get_balance_key(addr),
        );
        merge_system_contract_entry(
            fork_journaled_state,
            ACCOUNT_CODE_STORAGE_ADDRESS.to_address(),
            foundry_zksync_core::get_account_code_key(addr),
        );
        merge_system_contract_entry(
            fork_journaled_state,
            NONCE_HOLDER_ADDRESS.to_address(),
            foundry_zksync_core::get_nonce_key(addr),
        );

        if let Some(acc) = active_journaled_state.state.get(&addr) {
            merge_system_contract_entry(
                fork_journaled_state,
                KNOWN_CODES_STORAGE_ADDRESS.to_address(),
                U256::from_be_slice(&acc.info.code_hash.0[..]),
            );
        }

        // merge immutable storage.
        let immutable_simulator_addr = IMMUTABLE_SIMULATOR_STORAGE_ADDRESS.to_address();
        if let Some(immutable_storage_keys) = zk_state.persistent_immutable_keys.get(&addr) {
            for slot_key in immutable_storage_keys {
                merge_system_contract_entry(
                    fork_journaled_state,
                    immutable_simulator_addr,
                    *slot_key,
                );
            }
        }
    }
}
