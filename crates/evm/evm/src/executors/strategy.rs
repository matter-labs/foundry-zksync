use std::{any::Any, fmt::Debug, path::Path};

use alloy_primitives::{Address, U256};
use alloy_serde::OtherFields;
use eyre::Result;
use foundry_cheatcodes::strategy::{
    CheatcodeInspectorStrategy, EvmCheatcodeInspectorStrategyRunner,
};
use foundry_compilers::ProjectCompileOutput;
use foundry_config::Config;
use foundry_evm_core::{
    backend::{strategy::BackendStrategy, Backend, BackendResult, CowBackend},
    decode::RevertDecoder,
};
use foundry_linking::LinkerError;
use foundry_zksync_compilers::{
    compilers::{artifact_output::zk::ZkArtifactOutput, zksolc::ZkSolcCompiler},
    dual_compiled_contracts::DualCompiledContracts,
};
use revm::{
    primitives::{Env, EnvWithHandlerCfg, ResultAndState},
    DatabaseRef,
};

use crate::inspectors::InspectorStack;

use super::{EvmError, Executor};

mod libraries;
pub use libraries::*;

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

    fn get_balance(&self, executor: &mut Executor, address: Address) -> BackendResult<U256>;

    fn set_nonce(&self, executor: &mut Executor, address: Address, nonce: u64)
        -> BackendResult<()>;

    fn get_nonce(&self, executor: &mut Executor, address: Address) -> BackendResult<u64>;

    fn link(
        &self,
        ctx: &mut dyn ExecutorStrategyContext,
        config: &Config,
        root: &Path,
        input: &ProjectCompileOutput,
        deployer: Address,
    ) -> Result<LinkOutput, LinkerError>;

    /// Deploys a library, applying state changes
    fn deploy_library(
        &self,
        executor: &mut Executor,
        from: Address,
        input: DeployLibKind,
        value: U256,
        rd: Option<&RevertDecoder>,
    ) -> Result<Vec<DeployLibResult>, EvmError>;

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

    fn zksync_set_compilation_output(
        &self,
        _ctx: &mut dyn ExecutorStrategyContext,
        _output: ProjectCompileOutput<ZkSolcCompiler, ZkArtifactOutput>,
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

    fn get_balance(&self, executor: &mut Executor, address: Address) -> BackendResult<U256> {
        executor.get_balance(address)
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

    fn get_nonce(&self, executor: &mut Executor, address: Address) -> BackendResult<u64> {
        executor.get_nonce(address)
    }

    fn link(
        &self,
        _: &mut dyn ExecutorStrategyContext,
        _: &Config,
        root: &Path,
        input: &ProjectCompileOutput,
        deployer: Address,
    ) -> Result<LinkOutput, LinkerError> {
        self.link_impl(root, input, deployer)
    }

    /// Deploys a library, applying state changes
    fn deploy_library(
        &self,
        executor: &mut Executor,
        from: Address,
        kind: DeployLibKind,
        value: U256,
        rd: Option<&RevertDecoder>,
    ) -> Result<Vec<DeployLibResult>, EvmError> {
        self.deploy_library_impl(executor, from, kind, value, rd)
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
