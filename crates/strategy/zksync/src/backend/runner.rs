use std::any::Any;

use alloy_evm::eth::EthEvmContext;
use alloy_primitives::{Address, B256, U256};
use alloy_rpc_types::TransactionRequest;
use eyre::{Ok, Result};
use foundry_evm::{
    Env, InspectorExt,
    backend::{
        Backend, DatabaseExt, JournaledState,
        strategy::{BackendStrategyContext, BackendStrategyRunnerExt},
        update_state,
    },
};
use foundry_evm_core::{
    AsEnvMut as _,
    backend::{
        BackendInner, Fork, ForkDB, FoundryEvmInMemoryDB,
        strategy::{BackendStrategyForkInfo, BackendStrategyRunner, EvmBackendStrategyRunner},
    },
};
use revm::{
    DatabaseCommit,
    context::result::ResultAndState,
    primitives::{HashMap, HashSet},
};
use serde::{Deserialize, Serialize};

use crate::backend::{
    context::{ZksyncBackendStrategyContext, ZksyncInspectContext},
    merge::{ZksyncBackendMerge, ZksyncMergeState},
};
use foundry_evm_core::utils::configure_tx_req_env;

/// ZKsync implementation for [BackendStrategyRunner].
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ZksyncBackendStrategyRunner;

impl BackendStrategyRunner for ZksyncBackendStrategyRunner {
    fn inspect(
        &self,
        backend: &mut Backend,
        env: &mut Env,
        inspector: &mut dyn InspectorExt,
        inspect_ctx: Box<dyn Any>,
    ) -> Result<ResultAndState> {
        if !is_zksync_inspect_context(inspect_ctx.as_ref()) {
            return EvmBackendStrategyRunner.inspect(backend, env, inspector, inspect_ctx);
        }

        let inspect_ctx = get_inspect_context(inspect_ctx);
        let mut persisted_factory_deps =
            get_context(backend.strategy.context.as_mut()).persisted_factory_deps.clone();

        let result = foundry_zksync_core::vm::transact(
            Some(&mut persisted_factory_deps),
            Some(inspect_ctx.factory_deps),
            inspect_ctx.paymaster_data,
            env.evm_env.clone(),
            env.tx.clone(),
            &inspect_ctx.zk_env,
            backend,
            inspect_ctx.evm_interpreter,
        );

        let ctx = get_context(backend.strategy.context.as_mut());
        ctx.persisted_factory_deps = persisted_factory_deps;

        let mut evm_context =
            EthEvmContext::new(backend as &mut dyn DatabaseExt, env.evm_env.cfg_env.spec);

        // patch evm context with real caller
        evm_context.tx.caller = env.tx.caller;

        result.map(|(result, call_traces)| {
            inspector.trace_zksync(&mut evm_context, Box::new(call_traces), true);
            result
        })
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
        EvmBackendStrategyRunner.merge_journaled_state_data(
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
        EvmBackendStrategyRunner.merge_db_account_data(ctx, addr, active, fork_db);
        let ctx = get_context(ctx);
        let zk_state =
            &ZksyncMergeState { persistent_immutable_keys: &ctx.persistent_immutable_keys };
        ZksyncBackendMerge::merge_zk_account_data(addr, active, fork_db, zk_state);
    }

    fn transact_from_tx(
        &self,
        backend: &mut Backend,
        tx: &TransactionRequest,
        mut env: Env,
        journaled_state: &mut JournaledState,
        inspector: &mut dyn InspectorExt,
        inspect_ctx: Box<dyn Any>,
    ) -> eyre::Result<()> {
        if !is_zksync_inspect_context(inspect_ctx.as_ref()) {
            return EvmBackendStrategyRunner.transact_from_tx(
                backend,
                tx,
                env,
                journaled_state,
                inspector,
                inspect_ctx,
            );
        }

        backend.commit(journaled_state.state.clone());

        let res = {
            configure_tx_req_env(&mut env.as_env_mut(), tx, None)?;
            let env = env.clone();

            let inspect_ctx = get_inspect_context(inspect_ctx);
            let mut persisted_factory_deps =
                get_context(backend.strategy.context.as_mut()).persisted_factory_deps.clone();

            let result = foundry_zksync_core::vm::transact(
                Some(&mut persisted_factory_deps),
                Some(inspect_ctx.factory_deps),
                inspect_ctx.paymaster_data,
                env.evm_env.clone(),
                env.tx.clone(),
                &inspect_ctx.zk_env,
                backend,
                inspect_ctx.evm_interpreter,
            );

            let ctx = get_context(backend.strategy.context.as_mut());
            ctx.persisted_factory_deps = persisted_factory_deps;

            let mut evm_context =
                EthEvmContext::new(backend as &mut dyn DatabaseExt, env.evm_env.cfg_env.spec);

            // Patch evm context with real caller and real depth
            evm_context.tx.caller = env.tx.caller;
            evm_context.journaled_state.depth = journaled_state.depth + 1;

            result.map(|(result, call_traces)| {
                inspector.trace_zksync(&mut evm_context, Box::new(call_traces), true);
                result
            })?
        };

        backend.commit(res.state);
        update_state(&mut journaled_state.state, backend, None)?;

        Ok(())
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

    fn zksync_get_persisted_factory_deps(
        &self,
        ctx: &mut dyn BackendStrategyContext,
    ) -> HashMap<B256, Vec<u8>> {
        let ctx = get_context(ctx);
        ctx.persisted_factory_deps
            .clone()
            .into_iter()
            .map(|(hash, code)| (B256::from(hash.0), code))
            .collect()
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
