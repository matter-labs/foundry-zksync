use std::{any::Any, fmt::Debug};

use alloy_primitives::{Address, U256};
use alloy_serde::OtherFields;
use eyre::Result;
use foundry_cheatcodes::strategy::{
    CheatcodeInspectorStrategy, EvmCheatcodeInspectorStrategyRunner,
};
use foundry_evm_core::backend::{strategy::BackendStrategy, Backend, BackendResult, CowBackend};
use foundry_zksync_compilers::dual_compiled_contracts::DualCompiledContracts;
use revm::{
    primitives::{Env, EnvWithHandlerCfg, ResultAndState},
    DatabaseRef,
};

use crate::inspectors::InspectorStack;

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
    pub runner: &'static dyn ExecutorStrategyRunner,
    /// Strategy context.
    pub context: Box<dyn ExecutorStrategyContext>,
}

impl ExecutorStrategy {
    pub fn new_evm() -> Self {
        Self { runner: &EvmExecutorStrategyRunner, context: Box::new(()) }
    }
}

impl Clone for ExecutorStrategy {
    fn clone(&self) -> Self {
        Self { runner: self.runner, context: self.context.new_cloned() }
    }
}

pub trait ExecutorStrategyRunner: Debug + Send + Sync + ExecutorStrategyExt {
    fn set_balance(
        &self,
        executor: &mut Executor,
        address: Address,
        amount: U256,
    ) -> BackendResult<()>;

    fn set_nonce(&self, executor: &mut Executor, address: Address, nonce: u64)
        -> BackendResult<()>;

    /// Execute a transaction and *WITHOUT* applying state changes.
    fn call(
        &self,
        ctx: &dyn ExecutorStrategyContext,
        backend: &mut CowBackend<'_>,
        env: &mut EnvWithHandlerCfg,
        executor_env: &EnvWithHandlerCfg,
        inspector: &mut InspectorStack,
    ) -> Result<ResultAndState>;

    /// Execute a transaction and apply state changes.
    fn transact(
        &self,
        ctx: &mut dyn ExecutorStrategyContext,
        backend: &mut Backend,
        env: &mut EnvWithHandlerCfg,
        executor_env: &EnvWithHandlerCfg,
        inspector: &mut InspectorStack,
    ) -> Result<ResultAndState>;

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

    fn zksync_set_gas_per_pubdata(
        &self,
        _ctx: &mut dyn ExecutorStrategyContext,
        _gas_per_pubdata: u64,
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

    /// Sets the transaction context for the next [ExecutorStrategyRunner::call] or
    /// [ExecutorStrategyRunner::transact]. This selects whether to run the transaction on zkEVM
    /// or the EVM.
    /// This is based if the [OtherFields] contains
    /// [foundry_zksync_core::ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY] with
    /// [foundry_zksync_core::ZkTransactionMetadata].
    fn zksync_set_transaction_context(
        &self,
        _ctx: &mut dyn ExecutorStrategyContext,
        _other_fields: OtherFields,
    ) {
    }
}

/// Implements [ExecutorStrategyRunner] for EVM.
#[derive(Debug, Default, Clone)]
pub struct EvmExecutorStrategyRunner;

impl ExecutorStrategyRunner for EvmExecutorStrategyRunner {
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

    fn call(
        &self,
        _ctx: &dyn ExecutorStrategyContext,
        backend: &mut CowBackend<'_>,
        env: &mut EnvWithHandlerCfg,
        _executor_env: &EnvWithHandlerCfg,
        inspector: &mut InspectorStack,
    ) -> Result<ResultAndState> {
        backend.inspect(env, inspector, Box::new(()))
    }

    fn transact(
        &self,
        _ctx: &mut dyn ExecutorStrategyContext,
        backend: &mut Backend,
        env: &mut EnvWithHandlerCfg,
        _executor_env: &EnvWithHandlerCfg,
        inspector: &mut InspectorStack,
    ) -> Result<ResultAndState> {
        backend.inspect(env, inspector, Box::new(()))
    }

    fn new_backend_strategy(&self) -> BackendStrategy {
        BackendStrategy::new_evm()
    }

    fn new_cheatcode_inspector_strategy(
        &self,
        _ctx: &dyn ExecutorStrategyContext,
    ) -> CheatcodeInspectorStrategy {
        CheatcodeInspectorStrategy {
            runner: &EvmCheatcodeInspectorStrategyRunner,
            context: Box::new(()),
        }
    }
}

impl ExecutorStrategyExt for EvmExecutorStrategyRunner {}
