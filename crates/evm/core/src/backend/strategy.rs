use std::sync::{Arc, Mutex};

use crate::InspectorExt;

use super::{Backend, BackendInner, Fork, ForkDB, ForkType, FoundryEvmInMemoryDB};
use alloy_primitives::Address;
use eyre::Context;
use revm::{
    db::CacheDB,
    primitives::{EnvWithHandlerCfg, ResultAndState},
    DatabaseRef, JournaledState,
};
use serde::{Deserialize, Serialize};

pub struct BackendStrategyForkInfo<'a> {
    pub active_fork: Option<&'a Fork>,
    pub active_type: ForkType,
    pub target_type: ForkType,
}

pub trait BackendStrategy: std::fmt::Debug + Send + Sync + Default + Clone + Serialize + for<'a> Deserialize<'a> + 'static
where
    Self: Sized,
{
    fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::default()))
    }
    
    fn name(&self) -> &'static str;

    /// When creating or switching forks, we update the AccountInfo of the contract
    fn update_fork_db(
        &self,
        fork_info: BackendStrategyForkInfo<'_>,
        mem_db: &FoundryEvmInMemoryDB,
        backend_inner: &BackendInner,
        active_journaled_state: &mut JournaledState,
        target_fork: &mut Fork,
    );

    /// Executes the configured test call of the `env` without committing state changes.
    ///
    /// Note: in case there are any cheatcodes executed that modify the environment, this will
    /// update the given `env` with the new values.
    #[instrument(name = "inspect", level = "debug", skip_all)]
    fn inspect<I: InspectorExt>(
        &mut self,
        backend: &mut Backend<Self>,
        env: &mut EnvWithHandlerCfg,
        inspector: &mut I,
        _extra: Option<Vec<u8>>,
    ) -> eyre::Result<ResultAndState> {
        backend.initialize(env);
        let mut evm = crate::utils::new_evm_with_inspector(backend, env.clone(), inspector);

        let res = evm.transact().wrap_err("backend: failed while inspecting")?;

        env.env = evm.context.evm.inner.env;

        Ok(res)
    }
}

// struct _ObjectSafe(dyn BackendStrategy);

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct EvmBackendStrategy;

impl BackendStrategy for EvmBackendStrategy {
    fn name(&self) -> &'static str {
        "evm"
    }

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
}

impl EvmBackendStrategy {
    /// Merges the state of all `accounts` from the currently active db into the given `fork`
    pub(crate) fn update_fork_db_contracts(
        &self,
        fork_info: BackendStrategyForkInfo<'_>,
        mem_db: &FoundryEvmInMemoryDB,
        backend_inner: &BackendInner,
        active_journaled_state: &mut JournaledState,
        target_fork: &mut Fork,
    ) {
        let accounts = backend_inner.persistent_accounts.iter().copied();
        if let Some(db) = fork_info.active_fork.map(|f| &f.db) {
            EvmBackendMergeStrategy::merge_account_data(
                accounts,
                db,
                active_journaled_state,
                target_fork,
            )
        } else {
            EvmBackendMergeStrategy::merge_account_data(
                accounts,
                mem_db,
                active_journaled_state,
                target_fork,
            )
        }
    }
}
pub struct EvmBackendMergeStrategy;
impl EvmBackendMergeStrategy {
    /// Clones the data of the given `accounts` from the `active` database into the `fork_db`
    /// This includes the data held in storage (`CacheDB`) and kept in the `JournaledState`.
    pub fn merge_account_data<ExtDB: DatabaseRef>(
        accounts: impl IntoIterator<Item = Address>,
        active: &CacheDB<ExtDB>,
        active_journaled_state: &mut JournaledState,
        target_fork: &mut Fork,
    ) {
        for addr in accounts.into_iter() {
            merge_db_account_data(addr, active, &mut target_fork.db);
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

/// Clones the account data from the `active_journaled_state`  into the `fork_journaled_state`
pub fn merge_journaled_state_data(
    addr: Address,
    active_journaled_state: &JournaledState,
    fork_journaled_state: &mut JournaledState,
) {
    if let Some(mut acc) = active_journaled_state.state.get(&addr).cloned() {
        trace!(?addr, "updating journaled_state account data");
        if let Some(fork_account) = fork_journaled_state.state.get_mut(&addr) {
            // This will merge the fork's tracked storage with active storage and update values
            fork_account.storage.extend(std::mem::take(&mut acc.storage));
            // swap them so we can insert the account as whole in the next step
            std::mem::swap(&mut fork_account.storage, &mut acc.storage);
        }
        fork_journaled_state.state.insert(addr, acc);
    }
}

/// Clones the account data from the `active` db into the `ForkDB`
pub fn merge_db_account_data<ExtDB: DatabaseRef>(
    addr: Address,
    active: &CacheDB<ExtDB>,
    fork_db: &mut ForkDB,
) {
    let mut acc = if let Some(acc) = active.accounts.get(&addr).cloned() {
        acc
    } else {
        // Account does not exist
        return;
    };

    if let Some(code) = active.contracts.get(&acc.info.code_hash).cloned() {
        fork_db.contracts.insert(acc.info.code_hash, code);
    }

    if let Some(fork_account) = fork_db.accounts.get_mut(&addr) {
        // This will merge the fork's tracked storage with active storage and update values
        fork_account.storage.extend(std::mem::take(&mut acc.storage));
        // swap them so we can insert the account as whole in the next step
        std::mem::swap(&mut fork_account.storage, &mut acc.storage);
    }

    fork_db.accounts.insert(addr, acc);
}
