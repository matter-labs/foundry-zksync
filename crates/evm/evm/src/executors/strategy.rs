use std::{any::Any, fmt::Debug};

use alloy_primitives::{Address, U256};
use alloy_serde::OtherFields;
use eyre::{Context, Result};
use foundry_cheatcodes::strategy::{
    CheatcodeInspectorStrategy, EvmCheatcodeInspectorStrategyRunner,
};
use foundry_evm_core::{
    backend::{strategy::BackendStrategy, BackendResult, DatabaseExt},
    InspectorExt,
};
use foundry_zksync_compiler::DualCompiledContracts;
use revm::{
    primitives::{Env, EnvWithHandlerCfg, ResultAndState},
    DatabaseRef,
};

use super::Executor;

pub trait ExecutorStrategyContext: Debug + Send + Sync + Any {
    /// Clone the strategy context.
    fn new_cloned(&self) -> Box<dyn ExecutorStrategyContext>;
    /// Alias as immutable reference of [Any].
    fn as_any_ref(&self) -> &dyn Any;
    /// Alias as mutable reference of [Any].
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl ExecutorStrategyContext for () {
    fn new_cloned(&self) -> Box<dyn ExecutorStrategyContext> {
        Box::new(())
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Debug)]
pub struct ExecutorStrategy {
    /// Strategy runner.
    pub runner: Box<dyn ExecutorStrategyRunner>,
    /// Strategy context.
    pub context: Box<dyn ExecutorStrategyContext>,
}

impl ExecutorStrategy {
    pub fn new_evm() -> Self {
        Self { runner: Box::new(EvmExecutorStrategyRunner::default()), context: Box::new(()) }
    }
}

impl Clone for ExecutorStrategy {
    fn clone(&self) -> Self {
        Self { runner: self.runner.new_cloned(), context: self.context.new_cloned() }
    }
}

pub trait ExecutorStrategyRunner: Debug + Send + Sync + ExecutorStrategyExt {
    fn name(&self) -> &'static str;

    fn new_cloned(&self) -> Box<dyn ExecutorStrategyRunner>;

    fn set_balance(
        &self,
        executor: &mut Executor,
        address: Address,
        amount: U256,
    ) -> BackendResult<()>;

    fn set_nonce(&self, executor: &mut Executor, address: Address, nonce: u64)
        -> BackendResult<()>;

    fn set_inspect_context(&self, ctx: &mut dyn ExecutorStrategyContext, other_fields: OtherFields);

    fn call_inspect(
        &self,
        ctx: &dyn ExecutorStrategyContext,
        db: &mut dyn DatabaseExt,
        env: &mut EnvWithHandlerCfg,
        inspector: &mut dyn InspectorExt,
    ) -> eyre::Result<ResultAndState>;

    fn transact_inspect(
        &self,
        ctx: &mut dyn ExecutorStrategyContext,
        db: &mut dyn DatabaseExt,
        env: &mut EnvWithHandlerCfg,
        _executor_env: &EnvWithHandlerCfg,
        inspector: &mut dyn InspectorExt,
    ) -> eyre::Result<ResultAndState>;

    fn new_backend_strategy(&self) -> BackendStrategy;
    fn new_cheatcode_inspector_strategy(
        &self,
        ctx: &dyn ExecutorStrategyContext,
    ) -> foundry_cheatcodes::strategy::CheatcodeInspectorStrategy;

    // TODO perhaps need to create fresh strategies as well
}

/// Extended trait for ZKsync.
pub trait ExecutorStrategyExt {
    /// Set [DualCompiledContracts] on the context.
    fn zksync_set_dual_compiled_contracts(
        &self,
        _ctx: &mut dyn ExecutorStrategyContext,
        _dual_compiled_contracts: DualCompiledContracts,
    ) {
    }

    /// Set the fork environment on the context.
    fn zksync_set_fork_env(
        &self,
        _ctx: &mut dyn ExecutorStrategyContext,
        _fork_url: &str,
        _env: &Env,
    ) -> Result<()> {
        Ok(())
    }
}

/// Implements [ExecutorStrategyRunner] for EVM.
#[derive(Debug, Default, Clone)]
pub struct EvmExecutorStrategyRunner {}

impl ExecutorStrategyRunner for EvmExecutorStrategyRunner {
    fn name(&self) -> &'static str {
        "evm"
    }

    fn new_cloned(&self) -> Box<dyn ExecutorStrategyRunner> {
        Box::new(self.clone())
    }

    fn set_inspect_context(
        &self,
        _ctx: &mut dyn ExecutorStrategyContext,
        _other_fields: OtherFields,
    ) {
    }

    /// Executes the configured test call of the `env` without committing state changes.
    ///
    /// Note: in case there are any cheatcodes executed that modify the environment, this will
    /// update the given `env` with the new values.
    fn call_inspect(
        &self,
        _ctx: &dyn ExecutorStrategyContext,
        db: &mut dyn DatabaseExt,
        env: &mut EnvWithHandlerCfg,
        inspector: &mut dyn InspectorExt,
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
        &self,
        _ctx: &mut dyn ExecutorStrategyContext,
        db: &mut dyn DatabaseExt,
        env: &mut EnvWithHandlerCfg,
        _executor_env: &EnvWithHandlerCfg,
        inspector: &mut dyn InspectorExt,
    ) -> eyre::Result<ResultAndState> {
        let mut evm = crate::utils::new_evm_with_inspector(db, env.clone(), inspector);

        let res = evm.transact().wrap_err("backend: failed while inspecting")?;

        env.env = evm.context.evm.inner.env;

        Ok(res)
    }

    fn set_balance(
        &self,
        executor: &mut Executor,
        address: Address,
        amount: U256,
    ) -> BackendResult<()> {
        trace!(?address, ?amount, "setting account balance");
        let mut account = executor.backend().basic_ref(address)?.unwrap_or_default();
        account.balance = amount;
        executor.backend_mut().insert_account_info(address, account);

        Ok(())
    }

    fn set_nonce(
        &self,
        executor: &mut Executor,
        address: Address,
        nonce: u64,
    ) -> BackendResult<()> {
        let mut account = executor.backend().basic_ref(address)?.unwrap_or_default();
        account.nonce = nonce;
        executor.backend_mut().insert_account_info(address, account);

        Ok(())
    }

    fn new_backend_strategy(&self) -> BackendStrategy {
        BackendStrategy::new_evm()
    }

    fn new_cheatcode_inspector_strategy(
        &self,
        _ctx: &dyn ExecutorStrategyContext,
    ) -> CheatcodeInspectorStrategy {
        CheatcodeInspectorStrategy {
            runner: Box::new(EvmCheatcodeInspectorStrategyRunner::default()),
            context: Box::new(()),
        }
    }
}

impl ExecutorStrategyExt for EvmExecutorStrategyRunner {}

impl Clone for Box<dyn ExecutorStrategyRunner> {
    fn clone(&self) -> Self {
        self.new_cloned()
    }
}
