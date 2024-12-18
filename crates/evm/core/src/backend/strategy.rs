use std::fmt::Debug;

use super::{BackendInner, Fork, ForkDB, ForkType, FoundryEvmInMemoryDB};
use alloy_primitives::{Address, Log, U256};
use revm::{
    db::CacheDB,
    primitives::{EvmState, HashSet},
    DatabaseRef,
};

/// A backend-agnostic trait that defines the behavior of a journaled state.
pub trait JournaledState: Sized + Clone + Debug {
    // TODO: Probably should be VM-agnostic as well, e.g. we should not expose the state
    // directly, but rather a set of methods to interact with it.
    fn evm_state(&self) -> &EvmState;
    fn evm_state_mut(&mut self) -> &mut EvmState;

    fn logs(&self) -> &[Log];
    fn logs_mut(&mut self) -> &mut Vec<Log>;

    fn depth(&self) -> usize;
    fn set_depth(&mut self, depth: usize);

    fn load_account<DB: revm::Database>(
        &mut self,
        address: Address,
        db: &mut DB,
    ) -> Result<
        revm::interpreter::StateLoad<&mut revm::primitives::Account>,
        revm::primitives::EVMError<DB::Error>,
    >;

    fn touch(&mut self, address: &Address);
}

impl JournaledState for revm::JournaledState {
    fn evm_state(&self) -> &EvmState {
        &self.state
    }

    fn evm_state_mut(&mut self) -> &mut EvmState {
        &mut self.state
    }

    fn logs(&self) -> &[Log] {
        &self.logs
    }

    fn logs_mut(&mut self) -> &mut Vec<Log> {
        &mut self.logs
    }

    fn depth(&self) -> usize {
        self.depth
    }

    fn set_depth(&mut self, depth: usize) {
        self.depth = depth;
    }

    fn load_account<DB: revm::Database>(
        &mut self,
        address: Address,
        db: &mut DB,
    ) -> Result<
        revm::interpreter::StateLoad<&mut revm::primitives::Account>,
        revm::primitives::EVMError<DB::Error>,
    > {
        self.load_account(address, db)
    }

    fn touch(&mut self, address: &Address) {
        self.touch(address)
    }
}

pub struct BackendStrategyForkInfo<'a, S: BackendStrategy> {
    pub active_fork: Option<&'a Fork<S>>,
    pub active_type: ForkType,
    pub target_type: ForkType,
}

pub trait BackendStrategy: Debug + Send + Sync + Clone + Copy {
    type JournaledState: JournaledState;

    /// When creating or switching forks, we update the AccountInfo of the contract
    fn update_fork_db(
        fork_info: BackendStrategyForkInfo<'_, Self>,
        mem_db: &FoundryEvmInMemoryDB,
        backend_inner: &BackendInner<Self>,
        active_journaled_state: &mut Self::JournaledState,
        target_fork: &mut Fork<Self>,
    );

    /// Clones the account data from the `active_journaled_state` into the `fork_journaled_state`
    fn merge_journaled_state_data(
        addr: Address,
        active_journaled_state: &Self::JournaledState,
        fork_journaled_state: &mut Self::JournaledState,
    );

    fn merge_db_account_data(addr: Address, active: &ForkDB, fork_db: &mut ForkDB);

    fn new_journaled_state(backend_inner: &BackendInner<Self>) -> Self::JournaledState;
}

pub trait BackendStrategyExt: BackendStrategy {
    /// Saves the storage keys for immutable variables per address.
    ///
    /// These are required during fork to help merge the persisted addresses, as they are stored
    /// hashed so there is currently no way to retrieve all the address associated storage keys.
    /// We store all the storage keys here, even if the addresses are not marked persistent as
    /// they can be marked at a later stage as well.
    fn zksync_save_immutable_storage(&mut self, _addr: Address, _keys: HashSet<U256>) {}
}

#[derive(Debug, Default, Clone, Copy)]
pub struct EvmBackendStrategy;

impl BackendStrategy for EvmBackendStrategy {
    type JournaledState = revm::JournaledState;

    fn update_fork_db(
        fork_info: BackendStrategyForkInfo<'_, Self>,
        mem_db: &FoundryEvmInMemoryDB,
        backend_inner: &BackendInner,
        active_journaled_state: &mut Self::JournaledState,
        target_fork: &mut Fork,
    ) {
        Self::update_fork_db_contracts(
            fork_info,
            mem_db,
            backend_inner,
            active_journaled_state,
            target_fork,
        )
    }

    fn merge_journaled_state_data(
        addr: Address,
        active_journaled_state: &Self::JournaledState,
        fork_journaled_state: &mut Self::JournaledState,
    ) {
        EvmBackendMergeStrategy::merge_journaled_state_data(
            addr,
            active_journaled_state,
            fork_journaled_state,
        );
    }

    fn merge_db_account_data(addr: Address, active: &ForkDB, fork_db: &mut ForkDB) {
        EvmBackendMergeStrategy::merge_db_account_data(addr, active, fork_db);
    }

    fn new_journaled_state(backend_inner: &BackendInner) -> Self::JournaledState {
        revm::JournaledState::new(
            backend_inner.spec_id,
            backend_inner.precompiles().addresses().copied().collect(),
        )
    }
}

impl EvmBackendStrategy {
    /// Merges the state of all `accounts` from the currently active db into the given `fork`
    pub(crate) fn update_fork_db_contracts(
        fork_info: BackendStrategyForkInfo<'_, Self>,
        mem_db: &FoundryEvmInMemoryDB,
        backend_inner: &BackendInner,
        active_journaled_state: &mut revm::JournaledState,
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
        active_journaled_state: &mut revm::JournaledState,
        target_fork: &mut Fork,
    ) {
        for addr in accounts.into_iter() {
            Self::merge_db_account_data(addr, active, &mut target_fork.db);
            Self::merge_journaled_state_data(
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

    /// Clones the account data from the `active_journaled_state`  into the `fork_journaled_state`
    pub fn merge_journaled_state_data(
        addr: Address,
        active_journaled_state: &revm::JournaledState,
        fork_journaled_state: &mut revm::JournaledState,
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
}
