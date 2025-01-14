use std::path::Path;

use alloy_primitives::{keccak256, Address, Bytes, TxKind, B256, U256};
use alloy_rpc_types::serde_helpers::OtherFields;
use alloy_zksync::provider::{zksync_provider, ZksyncProvider};
use eyre::Result;

use foundry_common::ContractsByArtifact;
use foundry_compilers::{contracts::ArtifactContracts, Artifact, ProjectCompileOutput};
use foundry_config::Config;
use foundry_evm::{
    backend::{Backend, BackendResult, CowBackend},
    decode::RevertDecoder,
    executors::{
        strategy::{
            EvmExecutorStrategyRunner, ExecutorStrategy, ExecutorStrategyContext,
            ExecutorStrategyExt, ExecutorStrategyRunner, LinkOutput,
        },
        DeployResult, EvmError, Executor,
    },
    inspectors::InspectorStack,
};
use foundry_linking::{Linker, LinkerError, ZkLinker, ZkLinkerError};
use foundry_zksync_compilers::{
    compilers::{artifact_output::zk::ZkArtifactOutput, zksolc::ZkSolcCompiler},
    dual_compiled_contracts::{DualCompiledContract, DualCompiledContracts},
};
use foundry_zksync_core::{
    encode_create_params, hash_bytecode, vm::ZkEnv, ZkTransactionMetadata,
    ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY,
};
use revm::{
    primitives::{CreateScheme, Env, EnvWithHandlerCfg, ResultAndState},
    Database,
};
use zksync_types::H256;

use crate::{
    backend::{ZksyncBackendStrategyBuilder, ZksyncInspectContext},
    cheatcode::ZksyncCheatcodeInspectorStrategyBuilder,
};

/// Defines the context for [ZksyncExecutorStrategyRunner].
#[derive(Debug, Default, Clone)]
pub struct ZksyncExecutorStrategyContext {
    transaction_context: Option<ZkTransactionMetadata>,
    compilation_output: Option<ProjectCompileOutput<ZkSolcCompiler, ZkArtifactOutput>>,
    dual_compiled_contracts: DualCompiledContracts,
    zk_env: ZkEnv,
}

impl ExecutorStrategyContext for ZksyncExecutorStrategyContext {
    fn new_cloned(&self) -> Box<dyn ExecutorStrategyContext> {
        Box::new(self.clone())
    }

    fn as_any_ref(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Defines the [ExecutorStrategyRunner] strategy for ZKsync.
#[derive(Debug, Default, Clone)]
pub struct ZksyncExecutorStrategyRunner;

impl ZksyncExecutorStrategyRunner {
    fn set_deployment_nonce(
        executor: &mut Executor,
        address: Address,
        nonce: u64,
    ) -> BackendResult<()> {
        let (address, slot) = foundry_zksync_core::state::get_nonce_storage(address);
        // fetch the full nonce to preserve account's tx nonce
        let full_nonce = executor.backend.storage(address, slot)?;
        let full_nonce = foundry_zksync_core::state::parse_full_nonce(full_nonce);
        let new_full_nonce = foundry_zksync_core::state::new_full_nonce(full_nonce.tx_nonce, nonce);
        executor.backend.insert_account_storage(address, slot, new_full_nonce)?;

        Ok(())
    }
}

fn get_context_ref(ctx: &dyn ExecutorStrategyContext) -> &ZksyncExecutorStrategyContext {
    ctx.as_any_ref().downcast_ref().expect("expected ZksyncExecutorStrategyContext")
}

fn get_context(ctx: &mut dyn ExecutorStrategyContext) -> &mut ZksyncExecutorStrategyContext {
    ctx.as_any_mut().downcast_mut().expect("expected ZksyncExecutorStrategyContext")
}

impl ExecutorStrategyRunner for ZksyncExecutorStrategyRunner {
    fn set_balance(
        &self,
        executor: &mut Executor,
        address: Address,
        amount: U256,
    ) -> BackendResult<()> {
        EvmExecutorStrategyRunner.set_balance(executor, address, amount)?;

        let (address, slot) = foundry_zksync_core::state::get_balance_storage(address);
        executor.backend.insert_account_storage(address, slot, amount)?;

        Ok(())
    }

    fn get_balance(&self, executor: &mut Executor, address: Address) -> BackendResult<U256> {
        let (address, slot) = foundry_zksync_core::state::get_balance_storage(address);
        let balance = executor.backend.storage(address, slot)?;

        Ok(balance)
    }

    fn set_nonce(
        &self,
        executor: &mut Executor,
        address: Address,
        nonce: u64,
    ) -> BackendResult<()> {
        EvmExecutorStrategyRunner.set_nonce(executor, address, nonce)?;

        let (address, slot) = foundry_zksync_core::state::get_nonce_storage(address);
        // fetch the full nonce to preserve account's deployment nonce
        let full_nonce = executor.backend.storage(address, slot)?;
        let full_nonce = foundry_zksync_core::state::parse_full_nonce(full_nonce);
        let new_full_nonce =
            foundry_zksync_core::state::new_full_nonce(nonce, full_nonce.deploy_nonce);
        executor.backend.insert_account_storage(address, slot, new_full_nonce)?;

        Ok(())
    }

    fn get_nonce(&self, executor: &mut Executor, address: Address) -> BackendResult<u64> {
        let (address, slot) = foundry_zksync_core::state::get_nonce_storage(address);
        let full_nonce = executor.backend.storage(address, slot)?;
        let full_nonce = foundry_zksync_core::state::parse_full_nonce(full_nonce);

        Ok(full_nonce.tx_nonce)
    }

    fn link(
        &self,
        ctx: &mut dyn ExecutorStrategyContext,
        config: &Config,
        root: &Path,
        input: &ProjectCompileOutput,
        deployer: Address,
    ) -> Result<LinkOutput, LinkerError> {
        let evm_link = EvmExecutorStrategyRunner.link(ctx, config, root, input, deployer)?;

        let ctx = get_context(ctx);
        let Some(input) = ctx.compilation_output.as_ref() else {
            return Err(LinkerError::MissingTargetArtifact)
        };

        let contracts =
            input.artifact_ids().map(|(id, v)| (id.with_stripped_file_prefixes(root), v)).collect();

        let Ok(zksolc) = foundry_config::zksync::config_zksolc_compiler(config) else {
            tracing::error!("unable to determine zksolc compiler to be used for linking");
            // TODO(zk): better error
            return Err(LinkerError::CyclicDependency);
        };

        let linker = ZkLinker::new(root, contracts, zksolc);

        let zk_linker_error_to_linker = |zk_error| match zk_error {
            ZkLinkerError::Inner(err) => err,
            // TODO(zk): better error value
            ZkLinkerError::MissingLibraries(libs) => LinkerError::MissingLibraryArtifact {
                file: "libraries".to_string(),
                name: libs.len().to_string(),
            },
            ZkLinkerError::MissingFactoryDeps(libs) => LinkerError::MissingLibraryArtifact {
                file: "factoryDeps".to_string(),
                name: libs.len().to_string(),
            },
        };

        let foundry_linking::LinkOutput { libraries, libs_to_deploy: _ } = linker
            .zk_link_with_nonce_or_address(
                Default::default(),
                deployer,
                // NOTE(zk): match with EVM nonces as we will be doing a duplex deployment for
                // the libs
                0,
                linker.linker.contracts.keys(),
            )
            .map_err(zk_linker_error_to_linker)?;

        let mut linked_contracts = linker
            .zk_get_linked_artifacts(linker.linker.contracts.keys(), &libraries)
            .map_err(zk_linker_error_to_linker)?;

        let newly_linked_dual_compiled_contracts = linked_contracts
            .iter()
            .flat_map(|(needle, zk)| {
                evm_link
                    .linked_contracts
                    .iter()
                    .find(|(id, _)| id.source == needle.source && id.name == needle.name)
                    .map(|(_, evm)| (needle, zk, evm))
            })
            .filter(|(_, zk, evm)| zk.bytecode.is_some() && evm.bytecode.is_some())
            .map(|(id, linked_zk, evm)| {
                let (_, unlinked_zk_artifact) = input
                    .artifact_ids()
                    .find(|(contract_id, _)| {
                        contract_id.clone().with_stripped_file_prefixes(root) == id.clone()
                    })
                    .unwrap();
                let zk_bytecode = linked_zk.get_bytecode_bytes().unwrap();
                let zk_hash = hash_bytecode(&zk_bytecode);
                let evm = evm.get_bytecode_bytes().unwrap();
                let contract = DualCompiledContract {
                    name: id.name.clone(),
                    zk_bytecode_hash: zk_hash,
                    zk_deployed_bytecode: zk_bytecode.to_vec(),
                    // TODO(zk): retrieve unlinked factory deps (1.5.9)
                    zk_factory_deps: vec![zk_bytecode.to_vec()],
                    evm_bytecode_hash: B256::from_slice(&keccak256(evm.as_ref())[..]),
                    // TODO(zk): determine if this is ok, as it's
                    // not really used in dual compiled contracts
                    evm_deployed_bytecode: evm.to_vec(),
                    evm_bytecode: evm.to_vec(),
                };

                // populate factory deps that were already linked
                ctx.dual_compiled_contracts.extend_factory_deps_by_hash(
                    contract,
                    unlinked_zk_artifact.factory_dependencies.iter().flatten().map(|(_, hash)| {
                        H256::from_slice(alloy_primitives::hex::decode(hash).unwrap().as_slice())
                    }),
                )
            });

        ctx.dual_compiled_contracts
            .extend(newly_linked_dual_compiled_contracts.collect::<Vec<_>>());

        // Extend zk contracts with solc contracts as well. This is required for traces to
        // accurately detect contract names deployed in EVM mode, and when using
        // `vm.zkVmSkip()` cheatcode.
        linked_contracts.extend(evm_link.linked_contracts);

        Ok(LinkOutput {
            deployable_contracts: evm_link.deployable_contracts,
            revert_decoder: evm_link.revert_decoder,
            known_contracts: ContractsByArtifact::new(linked_contracts.clone()),
            linked_contracts,
            libs_to_deploy: evm_link.libs_to_deploy,
            libraries: evm_link.libraries,
        })
    }

    fn deploy_library(
        &self,
        executor: &mut Executor,
        from: Address,
        code: Bytes,
        value: U256,
        rd: Option<&RevertDecoder>,
    ) -> Result<Vec<DeployResult>, EvmError> {
        // sync deployer account info
        let nonce = EvmExecutorStrategyRunner.get_nonce(executor, from).expect("deployer to exist");
        let balance =
            EvmExecutorStrategyRunner.get_balance(executor, from).expect("deployer to exist");

        Self::set_deployment_nonce(executor, from, nonce).map_err(|err| eyre::eyre!(err))?;
        self.set_balance(executor, from, balance).map_err(|err| eyre::eyre!(err))?;
        tracing::debug!(?nonce, ?balance, sender = ?from, "deploying lib in EraVM");

        let mut evm_deployment = EvmExecutorStrategyRunner.deploy_library(
            executor,
            from,
            code.clone(),
            value,
            rd.clone(),
        )?;

        let ctx = get_context(executor.strategy.context.as_mut());

        // lookup dual compiled contract based on EVM bytecode
        let Some(dual_contract) = ctx.dual_compiled_contracts.find_by_evm_bytecode(code.as_ref())
        else {
            // we don't know what the equivalent zk contract would be
            return Ok(evm_deployment);
        };

        // no need for constructor args as it's a lib
        let create_params =
            encode_create_params(&CreateScheme::Create, dual_contract.zk_bytecode_hash, vec![]);

        // populate ctx.transaction_context with factory deps
        // we also populate the ctx so the deployment is executed
        // entirely in EraVM
        let factory_deps = ctx.dual_compiled_contracts.fetch_all_factory_deps(dual_contract);

        // persist existing paymaster data (TODO(zk): is this needed?)
        let paymaster_data =
            ctx.transaction_context.take().and_then(|metadata| metadata.paymaster_data);
        ctx.transaction_context = Some(ZkTransactionMetadata { factory_deps, paymaster_data });

        // eravm_env: call to ContractDeployer w/ properly encoded calldata
        let env = executor.build_test_env(
            from,
            // foundry_zksync_core::vm::runner::transact takes care of using the ContractDeployer
            // address
            TxKind::Create,
            create_params.into(),
            value,
        );

        executor.deploy_with_env(env, rd).map(move |dr| {
            evm_deployment.push(dr);
            evm_deployment
        })
    }

    fn new_backend_strategy(&self) -> foundry_evm_core::backend::strategy::BackendStrategy {
        foundry_evm_core::backend::strategy::BackendStrategy::new_zksync()
    }

    fn new_cheatcode_inspector_strategy(
        &self,
        ctx: &dyn ExecutorStrategyContext,
    ) -> foundry_cheatcodes::strategy::CheatcodeInspectorStrategy {
        let ctx = get_context_ref(ctx);
        foundry_cheatcodes::strategy::CheatcodeInspectorStrategy::new_zksync(
            ctx.dual_compiled_contracts.clone(),
            ctx.zk_env.clone(),
        )
    }

    fn call(
        &self,
        ctx: &dyn ExecutorStrategyContext,
        backend: &mut CowBackend<'_>,
        env: &mut EnvWithHandlerCfg,
        executor_env: &EnvWithHandlerCfg,
        inspector: &mut InspectorStack,
    ) -> eyre::Result<ResultAndState> {
        let ctx = get_context_ref(ctx);

        match ctx.transaction_context.as_ref() {
            None => EvmExecutorStrategyRunner.call(ctx, backend, env, executor_env, inspector),
            Some(zk_tx) => backend.inspect(
                env,
                inspector,
                Box::new(ZksyncInspectContext {
                    factory_deps: zk_tx.factory_deps.clone(),
                    paymaster_data: zk_tx.paymaster_data.clone(),
                    zk_env: ctx.zk_env.clone(),
                }),
            ),
        }
    }

    fn transact(
        &self,
        ctx: &mut dyn ExecutorStrategyContext,
        backend: &mut Backend,
        env: &mut EnvWithHandlerCfg,
        executor_env: &EnvWithHandlerCfg,
        inspector: &mut InspectorStack,
    ) -> eyre::Result<ResultAndState> {
        let ctx = get_context(ctx);

        match ctx.transaction_context.take() {
            None => EvmExecutorStrategyRunner.transact(ctx, backend, env, executor_env, inspector),
            Some(zk_tx) => {
                // apply fork-related env instead of cheatcode handler
                // since it won't be set by zkEVM
                env.block = executor_env.block.clone();
                env.tx.gas_price = executor_env.tx.gas_price;

                backend.inspect(
                    env,
                    inspector,
                    Box::new(ZksyncInspectContext {
                        factory_deps: zk_tx.factory_deps,
                        paymaster_data: zk_tx.paymaster_data,
                        zk_env: ctx.zk_env.clone(),
                    }),
                )
            }
        }
    }
}

impl ExecutorStrategyExt for ZksyncExecutorStrategyRunner {
    fn zksync_set_dual_compiled_contracts(
        &self,
        ctx: &mut dyn ExecutorStrategyContext,
        dual_compiled_contracts: DualCompiledContracts,
    ) {
        let ctx = get_context(ctx);
        ctx.dual_compiled_contracts = dual_compiled_contracts;
    }

    fn zksync_set_compilation_output(
        &self,
        ctx: &mut dyn ExecutorStrategyContext,
        output: ProjectCompileOutput<ZkSolcCompiler, ZkArtifactOutput>,
    ) {
        let ctx = get_context(ctx);
        ctx.compilation_output.replace(output);
    }

    fn zksync_set_fork_env(
        &self,
        ctx: &mut dyn ExecutorStrategyContext,
        fork_url: &str,
        env: &Env,
    ) -> Result<()> {
        let ctx = get_context(ctx);

        let provider = zksync_provider().with_recommended_fillers().on_http(fork_url.parse()?);
        let block_number = env.block.number.try_into()?;
        // TODO(zk): switch to getFeeParams call when it is implemented for anvil-zksync
        let maybe_block_details = tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(provider.get_block_details(block_number))
        })
        .ok()
        .flatten();

        if let Some(block_details) = maybe_block_details {
            ctx.zk_env = ZkEnv {
                l1_gas_price: block_details
                    .l1_gas_price
                    .try_into()
                    .expect("failed to convert l1_gas_price to u64"),
                fair_l2_gas_price: block_details
                    .l2_fair_gas_price
                    .try_into()
                    .expect("failed to convert fair_l2_gas_price to u64"),
                fair_pubdata_price: block_details
                    .fair_pubdata_price
                    // TODO(zk): None as a value might mean L1Pegged model
                    // we need to find out if it will ever be relevant to
                    // us
                    .unwrap_or_default()
                    .try_into()
                    .expect("failed to convert fair_pubdata_price to u64"),
            };
        }

        Ok(())
    }

    fn zksync_set_transaction_context(
        &self,
        ctx: &mut dyn ExecutorStrategyContext,
        other_fields: OtherFields,
    ) {
        let ctx = get_context(ctx);
        let transaction_context = try_get_zksync_transaction_metadata(&other_fields);
        ctx.transaction_context = transaction_context;
    }
}

pub fn try_get_zksync_transaction_metadata(
    other_fields: &OtherFields,
) -> Option<ZkTransactionMetadata> {
    other_fields
        .get_deserialized::<ZkTransactionMetadata>(ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY)
        .transpose()
        .ok()
        .flatten()
}

/// Create ZKsync strategy for [ExecutorStrategy].
pub trait ZksyncExecutorStrategyBuilder {
    /// Create new zksync strategy.
    fn new_zksync() -> Self;
}

impl ZksyncExecutorStrategyBuilder for ExecutorStrategy {
    fn new_zksync() -> Self {
        Self {
            runner: &ZksyncExecutorStrategyRunner,
            context: Box::new(ZksyncExecutorStrategyContext::default()),
        }
    }
}
