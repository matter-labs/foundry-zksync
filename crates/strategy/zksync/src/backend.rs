use std::collections::hash_map::Entry;

use alloy_primitives::{map::HashMap, Address, U256};
use foundry_evm::backend::strategy::BackendStrategyExt;
use foundry_evm_core::backend::{
    strategy::{
        BackendStrategy, BackendStrategyForkInfo, EvmBackendMergeStrategy, EvmBackendStrategy,
    },
    BackendInner, Fork, ForkDB, FoundryEvmInMemoryDB,
};
use foundry_zksync_core::{
    convert::ConvertH160, PaymasterParams, ACCOUNT_CODE_STORAGE_ADDRESS,
    IMMUTABLE_SIMULATOR_STORAGE_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS, L2_BASE_TOKEN_ADDRESS,
    NONCE_HOLDER_ADDRESS,
};
use revm::{db::CacheDB, primitives::HashSet, DatabaseRef, JournaledState};
use serde::{Deserialize, Serialize};
use tracing::trace;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ZksyncBackendStrategy {
    evm: EvmBackendStrategy,
    /// Store storage keys per contract address for immutable variables.
    persistent_immutable_keys: HashMap<Address, HashSet<U256>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ZkBackendInspectData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub factory_deps: Option<Vec<Vec<u8>>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub paymaster_data: Option<PaymasterParams>,

    pub use_evm: bool,
}

impl BackendStrategy for ZksyncBackendStrategy {
    fn name(&self) -> &'static str {
        "zk"
    }

    fn new_cloned(&self) -> Box<dyn BackendStrategy> {
        Box::new(self.clone())
    }

    /// When creating or switching forks, we update the AccountInfo of the contract.
    fn update_fork_db(
        &self,
        fork_info: BackendStrategyForkInfo<'_>,
        mem_db: &FoundryEvmInMemoryDB,
        backend_inner: &BackendInner,
        active_journaled_state: &mut JournaledState,
        target_fork: &mut Fork,
    ) {
        self.update_fork_db_contracts(
            fork_info,
            mem_db,
            backend_inner,
            active_journaled_state,
            target_fork,
        )
    }

    fn merge_journaled_state_data(
        &self,
        addr: Address,
        active_journaled_state: &JournaledState,
        fork_journaled_state: &mut JournaledState,
    ) {
        self.evm.merge_journaled_state_data(addr, active_journaled_state, fork_journaled_state);
        let zk_state =
            &ZksyncMergeState { persistent_immutable_keys: &self.persistent_immutable_keys };
        ZksyncBackendMerge::merge_zk_journaled_state_data(
            addr,
            active_journaled_state,
            fork_journaled_state,
            zk_state,
        );
    }

    fn merge_db_account_data(&self, addr: Address, active: &ForkDB, fork_db: &mut ForkDB) {
        self.evm.merge_db_account_data(addr, active, fork_db);
        let zk_state =
            &ZksyncMergeState { persistent_immutable_keys: &self.persistent_immutable_keys };
        ZksyncBackendMerge::merge_zk_account_data(addr, active, fork_db, zk_state);
    }
}

impl ZksyncBackendStrategy {
    /// Merges the state of all `accounts` from the currently active db into the given `fork`
    pub(crate) fn update_fork_db_contracts(
        &self,
        fork_info: BackendStrategyForkInfo<'_>,
        mem_db: &FoundryEvmInMemoryDB,
        backend_inner: &BackendInner,
        active_journaled_state: &mut JournaledState,
        target_fork: &mut Fork,
    ) {
        let _require_zk_storage_merge =
            fork_info.active_type.is_zk() && fork_info.target_type.is_zk();

        // Ignore EVM interoperatability and import everything
        // if !require_zk_storage_merge {
        //     return;
        // }

        let accounts = backend_inner.persistent_accounts.iter().copied();
        let zk_state =
            &ZksyncMergeState { persistent_immutable_keys: &self.persistent_immutable_keys };
        if let Some(db) = fork_info.active_fork.map(|f| &f.db) {
            ZksyncBackendMerge::merge_account_data(
                accounts,
                db,
                active_journaled_state,
                target_fork,
                zk_state,
            )
        } else {
            ZksyncBackendMerge::merge_account_data(
                accounts,
                mem_db,
                active_journaled_state,
                target_fork,
                zk_state,
            )
        }
    }
}

impl BackendStrategyExt for ZksyncBackendStrategy {
    fn zksync_save_immutable_storage(&mut self, addr: Address, keys: HashSet<U256>) {
        self.persistent_immutable_keys
            .entry(addr)
            .and_modify(|entry| entry.extend(&keys))
            .or_insert(keys);
    }
}

pub(crate) struct ZksyncBackendMerge;

/// Defines the zksync specific state to help during merge.
pub(crate) struct ZksyncMergeState<'a> {
    persistent_immutable_keys: &'a HashMap<Address, HashSet<U256>>,
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
    fn merge_zk_account_data<ExtDB: DatabaseRef>(
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
    fn merge_zk_journaled_state_data(
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
