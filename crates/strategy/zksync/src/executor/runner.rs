use std::{collections::HashMap, path::Path};

use alloy_primitives::{keccak256, Address, Bytes, TxKind, B256, U256};
use alloy_rpc_types::serde_helpers::OtherFields;
use alloy_zksync::{
    contracts::l2::contract_deployer::CONTRACT_DEPLOYER_ADDRESS,
    provider::{zksync_provider, ZksyncProvider},
};
use eyre::Result;
use foundry_linking::{
    LinkerError, ZkLinker, ZkLinkerError, DEPLOY_TIME_LINKING_ZKSOLC_MIN_VERSION,
};
use revm::{
    primitives::{CreateScheme, Env, EnvWithHandlerCfg, Output, ResultAndState},
    Database,
};
use tracing::debug;

use foundry_common::{ContractsByArtifact, TransactionMaybeSigned};
use foundry_compilers::{
    artifacts::CompactContractBytecodeCow, contracts::ArtifactContracts, info::ContractInfo,
    Artifact, ProjectCompileOutput,
};
use foundry_config::Config;
use foundry_evm::{
    backend::{Backend, BackendResult, CowBackend, DatabaseExt},
    decode::RevertDecoder,
    executors::{
        strategy::{
            DeployLibKind, DeployLibResult, EvmExecutorStrategyRunner, ExecutorStrategyContext,
            ExecutorStrategyExt, ExecutorStrategyRunner, LinkOutput,
        },
        DeployResult, EvmError, Executor,
    },
    inspectors::InspectorStack,
};
use foundry_zksync_compilers::{
    compilers::{artifact_output::zk::ZkArtifactOutput, zksolc::ZkSolcCompiler},
    dual_compiled_contracts::{DualCompiledContract, DualCompiledContracts},
};
use foundry_zksync_core::{
    encode_create_params, hash_bytecode, vm::ZkEnv, ZkTransactionMetadata,
    DEFAULT_CREATE2_DEPLOYER_ZKSYNC, ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY,
};

use crate::{
    backend::{ZksyncBackendStrategyBuilder, ZksyncInspectContext},
    cheatcode::ZksyncCheatcodeInspectorStrategyBuilder,
    executor::{try_get_zksync_transaction_metadata, ZksyncExecutorStrategyContext},
};

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
            return Err(LinkerError::MissingTargetArtifact);
        };

        // we don't strip here unlinke upstream due to
        // `input` being used later during linking
        // and that is unstripped
        let contracts: ArtifactContracts<CompactContractBytecodeCow<'_>> =
            input.artifact_ids().collect();

        let Ok(zksolc) = foundry_config::zksync::config_zksolc_compiler(config) else {
            tracing::error!("unable to determine zksolc compiler to be used for linking");
            // TODO(zk): better error
            return Err(LinkerError::CyclicDependency);
        };
        let version = zksolc.version().map_err(|_| LinkerError::CyclicDependency)?;

        let linker = ZkLinker::new(root, contracts.clone(), zksolc, input);

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
                linker.linker.contracts.keys(), // link everything
            )
            .map_err(zk_linker_error_to_linker)?;

        // if we have no libraries then no linking will happen
        // so we can skip the version check
        if !libraries.is_empty() {
            // TODO(zk): better error
            if version < DEPLOY_TIME_LINKING_ZKSOLC_MIN_VERSION {
                tracing::error!(
                    %version,
                    minimum_version = %DEPLOY_TIME_LINKING_ZKSOLC_MIN_VERSION,
                    "deploy-time linking not supported"
                );
                return Err(LinkerError::CyclicDependency);
            }
        }

        let linked_contracts = linker
            .zk_get_linked_artifacts(linker.linker.contracts.keys(), &libraries)
            .map_err(zk_linker_error_to_linker)?;

        let newly_linked_dual_compiled_contracts = linked_contracts
            .iter()
            .flat_map(|(needle, zk)| {
                // match EVM linking's prefix stripping
                let stripped = needle.clone().with_stripped_file_prefixes(root);
                evm_link
                    .linked_contracts
                    .iter()
                    .find(|(id, _)| id.source == stripped.source && id.name == stripped.name)
                    .map(|(_, evm)| (needle, stripped, zk, evm))
            })
            .filter(|(_, _, zk, evm)| zk.bytecode.is_some() && evm.bytecode.is_some())
            .map(|(unstripped_id, id, linked_zk, evm)| {
                let (_, unlinked_zk_artifact) = input
                    .artifact_ids()
                    .find(|(contract_id, _)| contract_id == unstripped_id)
                    .expect("unable to find original (pre-linking) artifact");
                let zk_bytecode =
                    linked_zk.get_bytecode_bytes().expect("no EraVM bytecode (or unlinked)");
                let zk_hash = hash_bytecode(&zk_bytecode);
                let evm_deployed =
                    evm.get_deployed_bytecode_bytes().expect("no EVM bytecode (or unlinked)");
                let evm_bytecode = evm.get_bytecode_bytes().expect("no EVM bytecode (or unlinked)");
                let contract_info = ContractInfo {
                    name: id.name.clone(),
                    path: Some(id.source.to_string_lossy().into_owned()),
                };
                let contract = DualCompiledContract {
                    zk_bytecode_hash: zk_hash,
                    zk_deployed_bytecode: zk_bytecode.to_vec(),
                    // rest of factory deps is populated later
                    zk_factory_deps: vec![zk_bytecode.to_vec()],
                    evm_bytecode_hash: B256::from_slice(&keccak256(evm_deployed.as_ref())[..]),
                    // TODO(zk): determine if this is ok, as it's
                    // not really used in dual compiled contracts
                    evm_deployed_bytecode: evm_deployed.to_vec(),
                    evm_bytecode: evm_bytecode.to_vec(),
                };

                let mut factory_deps = unlinked_zk_artifact.all_factory_deps().collect::<Vec<_>>();
                factory_deps.dedup();

                ((contract_info.clone(), contract), (contract_info, factory_deps))
            });

        let (new_contracts, new_contracts_deps): (Vec<_>, HashMap<_, _>) =
            newly_linked_dual_compiled_contracts.unzip();
        ctx.dual_compiled_contracts.extend(new_contracts);

        // now that we have an updated list of DualCompiledContracts
        // retrieve all the factory deps for a given contracts and store them
        new_contracts_deps.into_iter().for_each(|(info, deps)| {
            deps.into_iter().for_each(|dep| {
                let mut split = dep.split(':');
                let path = split.next().expect("malformed factory dep path");
                let name = split.next().expect("malformed factory dep name");

                let bytecode = ctx
                    .dual_compiled_contracts
                    .find(Some(path), Some(name))
                    .next()
                    .expect("unknown factory dep")
                    .zk_deployed_bytecode
                    .clone();

                ctx.dual_compiled_contracts.insert_factory_deps(&info, Some(bytecode));
            });
        });

        let linked_contracts: ArtifactContracts = contracts
            .into_iter()
            .map(|(id, art)| (id, foundry_compilers::artifacts::CompactContractBytecode::from(art)))
            // Extend original zk contracts with newly linked ones
            .chain(linked_contracts)
            // Extend zk contracts with solc contracts as well. This is required for traces to
            // accurately detect contract names deployed in EVM mode, and when using
            // `vm.zkVmSkip()` cheatcode.
            .chain(evm_link.linked_contracts)
            .collect();

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
        kind: DeployLibKind,
        value: U256,
        rd: Option<&RevertDecoder>,
    ) -> Result<Vec<DeployLibResult>, EvmError> {
        // sync deployer account info
        let nonce = EvmExecutorStrategyRunner.get_nonce(executor, from).expect("deployer to exist");
        let balance =
            EvmExecutorStrategyRunner.get_balance(executor, from).expect("deployer to exist");

        Self::set_deployment_nonce(executor, from, nonce).map_err(|err| eyre::eyre!(err))?;
        self.set_balance(executor, from, balance).map_err(|err| eyre::eyre!(err))?;
        tracing::debug!(?nonce, ?balance, sender = ?from, "deploying lib in EraVM");

        let mut evm_deployment =
            EvmExecutorStrategyRunner.deploy_library(executor, from, kind.clone(), value, rd)?;

        let ctx = get_context(executor.strategy.context.as_mut());

        let (code, create_scheme, to) = match kind {
            DeployLibKind::Create(bytes) => {
                (bytes, CreateScheme::Create, CONTRACT_DEPLOYER_ADDRESS)
            }
            DeployLibKind::Create2(salt, bytes) => (
                bytes,
                CreateScheme::Create2 { salt: salt.into() },
                DEFAULT_CREATE2_DEPLOYER_ZKSYNC,
            ),
        };

        // lookup dual compiled contract based on EVM bytecode
        let Some((_, dual_contract)) =
            ctx.dual_compiled_contracts.find_by_evm_bytecode(code.as_ref())
        else {
            // we don't know what the equivalent zk contract would be
            return Ok(evm_deployment);
        };

        // no need for constructor args as it's a lib
        let create_params: Bytes =
            encode_create_params(&create_scheme, dual_contract.zk_bytecode_hash, vec![]).into();

        // populate ctx.transaction_context with factory deps
        // we also populate the ctx so the deployment is executed
        // entirely in EraVM
        let factory_deps = ctx.dual_compiled_contracts.fetch_all_factory_deps(dual_contract);
        tracing::debug!(n_fdeps = factory_deps.len());

        // persist existing paymaster data (TODO(zk): is this needed?)
        let paymaster_data =
            ctx.transaction_context.take().and_then(|metadata| metadata.paymaster_data);
        let metadata = ZkTransactionMetadata { factory_deps, paymaster_data };
        ctx.transaction_context = Some(metadata.clone());

        let result = executor.transact_raw(from, to, create_params.clone(), value)?;
        let result = result.into_result(rd)?;

        let Some(Output::Create(_, Some(address))) = result.out else {
            return Err(eyre::eyre!(
                "Deployment succeeded, but no address was returned: {result:#?}"
            )
            .into());
        };

        // also mark this library as persistent, this will ensure that the state of the library is
        // persistent across fork swaps in forking mode
        executor.backend_mut().add_persistent_account(address);
        debug!(%address, "deployed contract");

        let mut request = TransactionMaybeSigned::new(Default::default());
        let unsigned = request.as_unsigned_mut().unwrap();
        unsigned.from = Some(from);
        unsigned.input = create_params.into();
        unsigned.nonce = Some(nonce);
        // we use the deployer here for consistency with linking
        unsigned.to = Some(TxKind::Call(to));
        unsigned.other.insert(
            ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY.to_string(),
            serde_json::to_value(metadata).expect("failed encoding json"),
        );

        // ignore all EVM broadcastables
        evm_deployment.iter_mut().for_each(|result| {
            result.tx.take();
        });
        evm_deployment.push(DeployLibResult {
            result: DeployResult { raw: result, address },
            tx: Some(request),
        });
        Ok(evm_deployment)
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
