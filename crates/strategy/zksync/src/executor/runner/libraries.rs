//! Contains various definitions and items related to deploy-time linking
//! for zksync

use std::{collections::HashMap, path::Path};

use alloy_primitives::{keccak256, Address, Bytes, TxKind, B256, U256};
use alloy_zksync::contracts::l2::contract_deployer::CONTRACT_DEPLOYER_ADDRESS;
use foundry_common::{ContractsByArtifact, TransactionMaybeSigned};
use foundry_compilers::{
    artifacts::CompactContractBytecodeCow, contracts::ArtifactContracts, info::ContractInfo,
    Artifact, ProjectCompileOutput,
};
use foundry_config::Config;
use foundry_evm::{
    backend::DatabaseExt,
    decode::RevertDecoder,
    executors::{
        strategy::{
            DeployLibKind, DeployLibResult, EvmExecutorStrategyRunner, ExecutorStrategyContext,
            ExecutorStrategyRunner, LinkOutput,
        },
        DeployResult, EvmError, Executor,
    },
};
use foundry_linking::{
    LinkerError, ZkLinker, ZkLinkerError, DEPLOY_TIME_LINKING_ZKSOLC_MIN_VERSION,
};
use foundry_zksync_compilers::dual_compiled_contracts::DualCompiledContract;
use foundry_zksync_core::{
    encode_create_params, hash_bytecode, ZkTransactionMetadata, DEFAULT_CREATE2_DEPLOYER_ZKSYNC,
    ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY,
};
use revm::primitives::{CreateScheme, Output};

use super::{get_context, ZksyncExecutorStrategyRunner};

impl ZksyncExecutorStrategyRunner {
    pub(super) fn link_impl(
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

        let Ok(zksolc_settings) = foundry_config::zksync::config_zksolc_settings(config) else {
            tracing::error!("unable to determine zksolc compiler to be used for linking");
            // TODO(zk): better error
            return Err(LinkerError::CyclicDependency);
        };
        let version = zksolc_settings.zksolc_version_ref();

        let linker = ZkLinker::new(root, contracts.clone(), zksolc_settings.zksolc_path(), input);

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
            if version < &DEPLOY_TIME_LINKING_ZKSOLC_MIN_VERSION {
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
                    .1
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

    pub(super) fn deploy_library_impl(
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
        tracing::debug!(%address, "deployed contract");

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
}
