use std::sync::{Arc, Mutex};

use alloy_primitives::{Address, U256};
use foundry_evm_core::{
    backend::{
        strategy::{
            merge_db_account_data, merge_journaled_state_data, BackendStrategy,
            BackendStrategyForkInfo,
        },
        Backend, BackendInner, Fork, ForkDB, FoundryEvmInMemoryDB,
    },
    InspectorExt,
};
use foundry_strategy::RunnerStrategy;
use foundry_zksync_core::{
    convert::ConvertH160, ACCOUNT_CODE_STORAGE_ADDRESS, H256, L2_BASE_TOKEN_ADDRESS,
    NONCE_HOLDER_ADDRESS,
};
use revm::{
    db::CacheDB,
    primitives::{EnvWithHandlerCfg, HashMap, ResultAndState},
    DatabaseRef, JournaledState,
};

#[derive(Debug)]
pub struct ZkBackendStrategy {
    persisted_factory_deps: HashMap<H256, Vec<u8>>,
}

impl BackendStrategy for ZkBackendStrategy {
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

    fn inspect<'a>(
        &mut self,
        backend: &'a mut Backend,
        env: &mut EnvWithHandlerCfg,
        _inspector: &mut dyn InspectorExt<&'a mut Backend>,
        extra: Option<Vec<u8>>,
    ) -> eyre::Result<ResultAndState> {
        backend.initialize(env);

        let factory_deps = serde_json::from_slice(&extra.unwrap()).unwrap();
        foundry_zksync_core::vm::transact(
            Some(&mut self.persisted_factory_deps),
            factory_deps,
            env,
            backend,
        )
    }
}

impl ZkBackendStrategy {
    /// Merges the state of all `accounts` from the currently active db into the given `fork`
    pub(crate) fn update_fork_db_contracts(
        &self,
        fork_info: BackendStrategyForkInfo<'_>,
        mem_db: &FoundryEvmInMemoryDB,
        backend_inner: &BackendInner,
        active_journaled_state: &mut JournaledState,
        target_fork: &mut Fork,
    ) {
        let require_zk_storage_merge =
            fork_info.active_type.is_zk() && fork_info.target_type.is_zk();
        let accounts = backend_inner.persistent_accounts.iter().copied();
        if let Some(db) = fork_info.active_fork.map(|f| &f.db) {
            ZkBackendMergeStrategy::merge_account_data(
                accounts,
                db,
                active_journaled_state,
                target_fork,
                require_zk_storage_merge,
            )
        } else {
            ZkBackendMergeStrategy::merge_account_data(
                accounts,
                mem_db,
                active_journaled_state,
                target_fork,
                require_zk_storage_merge,
            )
        }
    }
}

pub struct ZkBackendMergeStrategy;
impl ZkBackendMergeStrategy {
    /// Clones the data of the given `accounts` from the `active` database into the `fork_db`
    /// This includes the data held in storage (`CacheDB`) and kept in the `JournaledState`.
    pub fn merge_account_data<ExtDB: DatabaseRef>(
        accounts: impl IntoIterator<Item = Address>,
        active: &CacheDB<ExtDB>,
        active_journaled_state: &mut JournaledState,
        target_fork: &mut Fork,
        _require_zk_storage_merge: bool,
    ) {
        for addr in accounts.into_iter() {
            merge_db_account_data(addr, active, &mut target_fork.db);

            // We do not care about EVM interoperatability now, so always update zk storage
            // if require_zk_storage_merge {
            //     merge_zk_storage_account_data(addr, active, &mut target_fork.db);
            // }
            merge_zk_storage_account_data(addr, active, &mut target_fork.db);
            merge_journaled_state_data(
                addr,
                active_journaled_state,
                &mut target_fork.journaled_state,
            );
        }

        // need to mock empty journal entries in case the current checkpoint is higher than the
        // existing journal entries
        while active_journaled_state.journal.len() > target_fork.journaled_state.journal.len() {
            target_fork.journaled_state.journal.push(Default::default());
        }

        *active_journaled_state = target_fork.journaled_state.clone();
    }
}

/// Clones the zk account data from the `active` db into the `ForkDB`
fn merge_zk_storage_account_data<ExtDB: DatabaseRef>(
    addr: Address,
    active: &CacheDB<ExtDB>,
    fork_db: &mut ForkDB,
) {
    let mut merge_system_contract_entry = |system_contract: Address, slot: U256| {
        let mut acc = if let Some(acc) = active.accounts.get(&system_contract).cloned() {
            acc
        } else {
            // Account does not exist
            return;
        };

        let mut storage = HashMap::<U256, U256>::default();
        if let Some(value) = acc.storage.get(&slot) {
            storage.insert(slot, *value);
        }

        if let Some(fork_account) = fork_db.accounts.get_mut(&system_contract) {
            // This will merge the fork's tracked storage with active storage and update values
            fork_account.storage.extend(storage);
            // swap them so we can insert the account as whole in the next step
            std::mem::swap(&mut fork_account.storage, &mut acc.storage);
        } else {
            std::mem::swap(&mut storage, &mut acc.storage)
        }

        fork_db.accounts.insert(system_contract, acc);
    };

    merge_system_contract_entry(
        L2_BASE_TOKEN_ADDRESS.to_address(),
        foundry_zksync_core::get_balance_key(addr),
    );
    merge_system_contract_entry(
        ACCOUNT_CODE_STORAGE_ADDRESS.to_address(),
        foundry_zksync_core::get_account_code_key(addr),
    );
    merge_system_contract_entry(
        NONCE_HOLDER_ADDRESS.to_address(),
        foundry_zksync_core::get_nonce_key(addr),
    );
}

pub struct ZkRunnerStrategy {
    pub backend: Arc<Mutex<dyn BackendStrategy>>,
}
impl Default for ZkRunnerStrategy {
    fn default() -> Self {
        Self {
            backend: Arc::new(Mutex::new(ZkBackendStrategy {
                persisted_factory_deps: Default::default(),
            })),
        }
    }
}
impl RunnerStrategy for ZkRunnerStrategy {
    fn name(&self) -> &'static str {
        "zk"
    }

    fn backend_strategy(&self) -> Arc<Mutex<dyn BackendStrategy>> {
        self.backend.clone()
    }
}
