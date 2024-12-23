use std::{any::Any, collections::hash_map::Entry};

use alloy_primitives::{map::HashMap, Address, U256};
use eyre::Result;
use foundry_evm::{
    backend::{
        strategy::{BackendStrategy, BackendStrategyContext, BackendStrategyRunnerExt},
        Backend,
    },
    InspectorExt,
};
use foundry_evm_core::backend::{
    strategy::{
        BackendStrategyForkInfo, BackendStrategyRunner, EvmBackendMergeStrategy,
        EvmBackendStrategyRunner,
    },
    BackendInner, Fork, ForkDB, FoundryEvmInMemoryDB,
};
use foundry_zksync_core::{
    convert::ConvertH160, vm::ZkEnv, PaymasterParams, ACCOUNT_CODE_STORAGE_ADDRESS,
    IMMUTABLE_SIMULATOR_STORAGE_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS, L2_BASE_TOKEN_ADDRESS,
    NONCE_HOLDER_ADDRESS,
};
use revm::{
    db::CacheDB,
    primitives::{EnvWithHandlerCfg, HashSet, ResultAndState},
    DatabaseRef, JournaledState,
};
use serde::{Deserialize, Serialize};
use tracing::trace;
use zksync_types::H256;

/// Represents additional data for ZK transactions.
#[derive(Clone, Debug, Default)]
pub struct ZksyncInspectContext {
    /// Factory Deps for ZK transactions.
    pub factory_deps: Vec<Vec<u8>>,
    /// Paymaster data for ZK transactions.
    pub paymaster_data: Option<PaymasterParams>,
    /// Zksync environment.
    pub zk_env: ZkEnv,
}

/// Context for [ZksyncBackendStrategyRunner].
#[derive(Debug, Default, Clone)]
pub struct ZksyncBackendStrategyContext {
    /// Store storage keys per contract address for immutable variables.
    persistent_immutable_keys: HashMap<Address, HashSet<U256>>,
    /// Store persisted factory dependencies.
    persisted_factory_deps: HashMap<H256, Vec<u8>>,
}

impl BackendStrategyContext for ZksyncBackendStrategyContext {
    fn new_cloned(&self) -> Box<dyn BackendStrategyContext> {
        Box::new(self.clone())
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// ZKsync implementation for [BackendStrategyRunner].
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ZksyncBackendStrategyRunner {
    evm: EvmBackendStrategyRunner,
}

impl BackendStrategyRunner for ZksyncBackendStrategyRunner {
    fn name(&self) -> &'static str {
        "zk"
    }

    fn new_cloned(&self) -> Box<dyn BackendStrategyRunner> {
        Box::new(self.clone())
    }

    fn inspect(
        &self,
        backend: &mut Backend,
        env: &mut EnvWithHandlerCfg,
        inspector: &mut dyn InspectorExt,
        inspect_ctx: Box<dyn Any>,
    ) -> Result<ResultAndState> {
        if !is_zksync_cainspect_context(&inspect_ctx) {
            return self.evm.inspect(backend, env, inspector, inspect_ctx);
        }

        let inspect_ctx = get_inspect_context(inspect_ctx);
        let mut persisted_factory_deps =
            get_context(backend.strategy.context.as_mut()).persisted_factory_deps.clone();

        let result = foundry_zksync_core::vm::transact(
            Some(&mut persisted_factory_deps),
            Some(inspect_ctx.factory_deps),
            inspect_ctx.paymaster_data,
            env,
            &inspect_ctx.zk_env,
            backend,
        );

        let ctx = get_context(backend.strategy.context.as_mut());
        ctx.persisted_factory_deps = persisted_factory_deps;

        result
    }

    /// When creating or switching forks, we update the AccountInfo of the contract.
    fn update_fork_db(
        &self,
        ctx: &mut dyn BackendStrategyContext,
        fork_info: BackendStrategyForkInfo<'_>,
        mem_db: &FoundryEvmInMemoryDB,
        backend_inner: &BackendInner,
        active_journaled_state: &mut JournaledState,
        target_fork: &mut Fork,
    ) {
        let ctx = get_context(ctx);
        self.update_fork_db_contracts(
            ctx,
            fork_info,
            mem_db,
            backend_inner,
            active_journaled_state,
            target_fork,
        )
    }

    fn merge_journaled_state_data(
        &self,
        ctx: &mut dyn BackendStrategyContext,
        addr: Address,
        active_journaled_state: &JournaledState,
        fork_journaled_state: &mut JournaledState,
    ) {
        self.evm.merge_journaled_state_data(
            ctx,
            addr,
            active_journaled_state,
            fork_journaled_state,
        );
        let ctx = get_context(ctx);
        let zk_state =
            &ZksyncMergeState { persistent_immutable_keys: &ctx.persistent_immutable_keys };
        ZksyncBackendMerge::merge_zk_journaled_state_data(
            addr,
            active_journaled_state,
            fork_journaled_state,
            zk_state,
        );
    }

    fn merge_db_account_data(
        &self,
        ctx: &mut dyn BackendStrategyContext,
        addr: Address,
        active: &ForkDB,
        fork_db: &mut ForkDB,
    ) {
        self.evm.merge_db_account_data(ctx, addr, active, fork_db);
        let ctx = get_context(ctx);
        let zk_state =
            &ZksyncMergeState { persistent_immutable_keys: &ctx.persistent_immutable_keys };
        ZksyncBackendMerge::merge_zk_account_data(addr, active, fork_db, zk_state);
    }
}

impl ZksyncBackendStrategyRunner {
    /// Merges the state of all `accounts` from the currently active db into the given `fork`
    pub(crate) fn update_fork_db_contracts(
        &self,
        ctx: &mut ZksyncBackendStrategyContext,
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
            &ZksyncMergeState { persistent_immutable_keys: &ctx.persistent_immutable_keys };
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

impl BackendStrategyRunnerExt for ZksyncBackendStrategyRunner {
    fn zksync_save_immutable_storage(
        &self,
        ctx: &mut dyn BackendStrategyContext,
        addr: Address,
        keys: HashSet<U256>,
    ) {
        let ctx = get_context(ctx);
        ctx.persistent_immutable_keys
            .entry(addr)
            .and_modify(|entry| entry.extend(&keys))
            .or_insert(keys);
    }
}

/// Create ZKsync strategy for [BackendStrategy].
pub trait ZksyncBackendStrategyBuilder {
    /// Create new zksync strategy.
    fn new_zksync() -> Self;
}

impl ZksyncBackendStrategyBuilder for BackendStrategy {
    fn new_zksync() -> Self {
        Self {
            runner: Box::new(ZksyncBackendStrategyRunner::default()),
            context: Box::new(ZksyncBackendStrategyContext::default()),
        }
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

fn get_context(ctx: &mut dyn BackendStrategyContext) -> &mut ZksyncBackendStrategyContext {
    ctx.as_any_mut().downcast_mut().expect("expected ZksyncBackendStrategyContext")
}

fn get_inspect_context(ctx: Box<dyn Any>) -> Box<ZksyncInspectContext> {
    ctx.downcast().expect("expected ZksyncInspectContext")
}

fn is_zksync_inspect_context(ctx: &dyn Any) -> bool {
    ctx.downcast_ref::<ZksyncInspectContext>().is_some()
}
