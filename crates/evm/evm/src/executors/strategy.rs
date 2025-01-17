use std::{any::Any, borrow::Borrow, collections::BTreeMap, fmt::Debug, path::Path};

use alloy_json_abi::JsonAbi;
use alloy_primitives::{Address, Bytes, TxKind, B256, U256};
use alloy_serde::OtherFields;
use eyre::{Context, Result};
use foundry_cheatcodes::strategy::{
    CheatcodeInspectorStrategy, EvmCheatcodeInspectorStrategyRunner,
};
use foundry_common::{ContractsByArtifact, TestFunctionExt, TransactionMaybeSigned};
use foundry_compilers::{
    artifacts::Libraries, contracts::ArtifactContracts, Artifact, ArtifactId, ProjectCompileOutput,
};
use foundry_config::Config;
use foundry_evm_core::{
    backend::{strategy::BackendStrategy, Backend, BackendResult, CowBackend, DatabaseExt},
    decode::RevertDecoder,
};
use foundry_linking::{Linker, LinkerError};
use foundry_zksync_compilers::{
    compilers::{artifact_output::zk::ZkArtifactOutput, zksolc::ZkSolcCompiler},
    dual_compiled_contracts::DualCompiledContracts,
};
use revm::{
    primitives::{Env, EnvWithHandlerCfg, Output, ResultAndState},
    DatabaseRef,
};

use crate::inspectors::InspectorStack;

use super::{DeployResult, EvmError, Executor};

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

pub struct LinkOutput {
    pub deployable_contracts: BTreeMap<ArtifactId, (JsonAbi, Bytes)>,
    pub revert_decoder: RevertDecoder,
    pub linked_contracts: ArtifactContracts,
    pub known_contracts: ContractsByArtifact,
    pub libs_to_deploy: Vec<Bytes>,
    pub libraries: Libraries,
}

/// Type of library deployment
#[derive(Debug, Clone)]
pub enum DeployLibKind {
    /// CREATE(bytecode)
    Create(Bytes),

    /// CREATE2(salt, bytecode)
    Create2(B256, Bytes),
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
    ) -> Result<Vec<(DeployResult, TransactionMaybeSigned)>, EvmError>;

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
        let contracts =
            input.artifact_ids().map(|(id, v)| (id.with_stripped_file_prefixes(root), v)).collect();
        let linker = Linker::new(root, contracts);

        // Build revert decoder from ABIs of all artifacts.
        let abis = linker
            .contracts
            .iter()
            .filter_map(|(_, contract)| contract.abi.as_ref().map(|abi| abi.borrow()));
        let revert_decoder = RevertDecoder::new().with_abis(abis);

        let foundry_linking::LinkOutput { libraries, libs_to_deploy } = linker
            .link_with_nonce_or_address(Default::default(), deployer, 0, linker.contracts.keys())?;

        let linked_contracts = linker.get_linked_artifacts(&libraries)?;

        // Create a mapping of name => (abi, deployment code, Vec<library deployment code>)
        let mut deployable_contracts = BTreeMap::default();
        for (id, contract) in linked_contracts.iter() {
            let Some(abi) = &contract.abi else { continue };

            // if it's a test, link it and add to deployable contracts
            if abi.constructor.as_ref().map(|c| c.inputs.is_empty()).unwrap_or(true)
                && abi.functions().any(|func| func.name.is_any_test())
            {
                let Some(bytecode) =
                    contract.get_bytecode_bytes().map(|b| b.into_owned()).filter(|b| !b.is_empty())
                else {
                    continue;
                };

                deployable_contracts.insert(id.clone(), (abi.clone(), bytecode));
            }
        }

        let known_contracts = ContractsByArtifact::new(linked_contracts.clone());

        Ok(LinkOutput {
            deployable_contracts,
            revert_decoder,
            linked_contracts,
            known_contracts,
            libs_to_deploy,
            libraries,
        })
    }

    /// Deploys a library, applying state changes
    fn deploy_library(
        &self,
        executor: &mut Executor,
        from: Address,
        kind: DeployLibKind,
        value: U256,
        rd: Option<&RevertDecoder>,
    ) -> Result<Vec<(DeployResult, TransactionMaybeSigned)>, EvmError> {
        let nonce = self.get_nonce(executor, from).context("retrieving sender nonce")?;

        match kind {
            DeployLibKind::Create(code) => {
                executor.deploy(from, code.clone(), value, rd).map(|dr| {
                    let mut request = TransactionMaybeSigned::new(Default::default());
                    let unsigned = request.as_unsigned_mut().unwrap();
                    unsigned.from = Some(from);
                    unsigned.input = code.into();
                    unsigned.nonce = Some(nonce);

                    vec![(dr, request)]
                })
            }
            DeployLibKind::Create2(salt, code) => {
                let create2_deployer = executor.create2_deployer();
                let calldata: Bytes = [salt.as_ref(), code.as_ref()].concat().into();
                let result =
                    executor.transact_raw(from, create2_deployer, calldata.clone(), value)?;
                let result = result.into_result(rd)?;

                let Some(Output::Create(_, Some(address))) = result.out else {
                    return Err(eyre::eyre!(
                        "Deployment succeeded, but no address was returned: {result:#?}"
                    )
                    .into());
                };

                // also mark this library as persistent, this will ensure that the state of the
                // library is persistent across fork swaps in forking mode
                executor.backend_mut().add_persistent_account(address);
                debug!(%address, "deployed contract with create2");

                let mut request = TransactionMaybeSigned::new(Default::default());
                let unsigned = request.as_unsigned_mut().unwrap();
                unsigned.from = Some(from);
                unsigned.input = calldata.into();
                unsigned.nonce = Some(nonce);
                unsigned.to = Some(TxKind::Call(create2_deployer));

                // manually increase nonce when performing CALLs
                executor
                    .set_nonce(from, nonce + 1)
                    .context("increasing nonce after CREATE2 deployment")?;

                Ok(vec![(DeployResult { raw: result, address }, request)])
            }
        }
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
