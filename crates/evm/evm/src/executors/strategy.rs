use std::fmt::Debug;

use alloy_primitives::{Address, U256};
use alloy_serde::OtherFields;
use eyre::{Context, Result};
use foundry_cheatcodes::strategy::{CheatcodeInspectorStrategyExt, EvmCheatcodeInspectorStrategy};
use foundry_evm_core::{
    backend::{
        strategy::{BackendStrategy, EvmBackendStrategy},
        Backend, BackendResult, DatabaseExt,
    },
    InspectorExt,
};
use foundry_zksync_compiler::DualCompiledContracts;
use revm::{
    primitives::{Env, EnvWithHandlerCfg, ResultAndState},
    DatabaseRef,
};

pub trait ExecutorStrategy: Debug + Send + Sized + 'static {
    type BackendStrategy: BackendStrategy;
    type ExecutorContext: Debug + Clone + Default + Send + Sync;

    fn backend_ctx(
        executor_ctx: &Self::ExecutorContext,
    ) -> <Self::BackendStrategy as BackendStrategy>::BackendContext;

    fn set_balance(db: &mut dyn DatabaseExt, address: Address, amount: U256) -> BackendResult<()>;

    fn set_nonce(db: &mut dyn DatabaseExt, address: Address, nonce: u64) -> BackendResult<()>;

    fn call_inspect(
        db: &mut dyn DatabaseExt,
        env: &mut EnvWithHandlerCfg,
        inspector: &mut dyn InspectorExt,
        ctx: &mut Self::ExecutorContext,
    ) -> eyre::Result<ResultAndState>;

    fn transact_inspect(
        db: &mut dyn DatabaseExt,
        env: &mut EnvWithHandlerCfg,
        _executor_env: &EnvWithHandlerCfg,
        inspector: &mut dyn InspectorExt,
        ctx: &mut Self::ExecutorContext,
    ) -> eyre::Result<ResultAndState>;

    fn new_cheatcode_inspector_strategy(
        ctx: &Self::ExecutorContext,
    ) -> Box<dyn CheatcodeInspectorStrategyExt>;

    // TODO perhaps need to create fresh strategies as well
}

// pub trait ExecutorStrategyExt: ExecutorStrategy {
//     fn new_cloned_ext(&self) -> Box<dyn ExecutorStrategyExt>;

//     fn zksync_set_dual_compiled_contracts(&self, _dual_compiled_contracts: DualCompiledContracts)
// {}

//     fn zksync_set_fork_env(&self, _fork_url: &str, _env: &Env) -> Result<()> {
//         Ok(())
//     }
// }

#[derive(Debug, Default, Clone)]
pub struct EvmExecutorStrategy {}

impl ExecutorStrategy for EvmExecutorStrategy {
    type BackendStrategy = EvmBackendStrategy;
    type ExecutorContext = ();

    fn backend_ctx(
        _executor_ctx: &Self::ExecutorContext,
    ) -> <Self::BackendStrategy as BackendStrategy>::BackendContext {
    }

    /// Executes the configured test call of the `env` without committing state changes.
    ///
    /// Note: in case there are any cheatcodes executed that modify the environment, this will
    /// update the given `env` with the new values.
    fn call_inspect(
        db: &mut dyn DatabaseExt,
        env: &mut EnvWithHandlerCfg,
        inspector: &mut dyn InspectorExt,
        ctx: &mut Self::ExecutorContext,
    ) -> eyre::Result<ResultAndState> {
        let mut evm = crate::utils::new_evm_with_inspector(db, env.clone(), inspector);

        let res = evm.transact().wrap_err("backend: failed while inspecting")?;

        env.env = evm.context.evm.inner.env;

        Ok(res)
    }

    /// Executes the configured test call of the `env` without committing state changes.
    /// Modifications to the state are however allowed.
    ///
    /// Note: in case there are any cheatcodes executed that modify the environment, this will
    /// update the given `env` with the new values.
    fn transact_inspect(
        db: &mut dyn DatabaseExt,
        env: &mut EnvWithHandlerCfg,
        _executor_env: &EnvWithHandlerCfg,
        inspector: &mut dyn InspectorExt,
        ctx: &mut Self::ExecutorContext,
    ) -> eyre::Result<ResultAndState> {
        let mut evm = crate::utils::new_evm_with_inspector(db, env.clone(), inspector);

        let res = evm.transact().wrap_err("backend: failed while inspecting")?;

        env.env = evm.context.evm.inner.env;

        Ok(res)
    }

    fn set_balance(db: &mut dyn DatabaseExt, address: Address, amount: U256) -> BackendResult<()> {
        trace!(?address, ?amount, "setting account balance");
        let mut account = db.basic_ref(address)?.unwrap_or_default();
        account.balance = amount;
        db.insert_account_info(address, account);

        Ok(())
    }

    fn set_nonce(db: &mut dyn DatabaseExt, address: Address, nonce: u64) -> BackendResult<()> {
        let mut account = db.basic_ref(address)?.unwrap_or_default();
        account.nonce = nonce;
        db.insert_account_info(address, account);

        Ok(())
    }

    fn new_cheatcode_inspector_strategy(
        _ctx: &Self::ExecutorContext,
    ) -> Box<dyn CheatcodeInspectorStrategyExt> {
        Box::new(EvmCheatcodeInspectorStrategy)
    }
}
