use std::collections::HashMap;

use alloy_primitives::{keccak256, B256};
use eyre::{Context, Result};
use foundry_compilers::{
    artifacts::{CompactContractBytecode, CompactContractBytecodeCow, Libraries},
    contracts::ArtifactContracts,
    info::ContractInfo,
    Artifact,
};
use foundry_linking::{ZkLinker, DEPLOY_TIME_LINKING_ZKSOLC_MIN_VERSION};
use foundry_zksync_compilers::dual_compiled_contracts::DualCompiledContract;
use foundry_zksync_core::{hash_bytecode, DEFAULT_CREATE2_DEPLOYER_ZKSYNC};

use crate::ScriptConfig;

use super::BuildData;

impl BuildData {
    fn get_zk_linker(&self, script_config: &ScriptConfig) -> Result<ZkLinker<'_>> {
        let zksolc_settings = foundry_config::zksync::config_zksolc_settings(&script_config.config)
            .context("retrieving zksolc compiler to be used for linking")?;
        let version = zksolc_settings.zksolc_version_ref();

        let Some(input) = self.zk_output.as_ref() else {
            eyre::bail!("unable to link zk artifacts if no zk compilation output is provided")
        };

        let linker = ZkLinker::new(
            self.project_root.clone(),
            input.artifact_ids().collect(),
            zksolc_settings.zksolc_path(),
            input,
        );

        let mut libs = Default::default();
        linker.zk_collect_dependencies(&self.target, &mut libs, None)?;

        // if there are no no libs, no linking will happen
        // so we can skip version check
        if !libs.is_empty() && version < &DEPLOY_TIME_LINKING_ZKSOLC_MIN_VERSION {
            eyre::bail!(
                "deploy-time linking not supported. minimum: {}, given: {}",
                DEPLOY_TIME_LINKING_ZKSOLC_MIN_VERSION,
                &version
            );
        }

        Ok(linker)
    }

    /// Will attempt linking via `zksolc`
    ///
    /// Will attempt linking with a CREATE2 deployer if possible first, otherwise
    /// just using CREATE.
    /// After linking is done it will update the list of `DualCompiledContracts` with
    /// the newly linked contracts (and their EVM equivalent).
    /// Finally, return the list of known contracts
    ///
    /// If compilation for zksync is not enabled will return the
    /// given EVM linked artifacts
    pub(super) async fn zk_link(
        &mut self,
        script_config: &ScriptConfig,
        known_libraries: Libraries,
        evm_linked_contracts: ArtifactContracts,
        use_create2: bool,
    ) -> Result<ArtifactContracts> {
        if !script_config.config.zksync.should_compile() {
            return Ok(evm_linked_contracts);
        }

        let Some(input) = self.zk_output.as_ref() else {
            eyre::bail!("unable to link zk artifacts if no zk compilation output is provided");
        };

        let mut dual_compiled_contracts = self.dual_compiled_contracts.take().unwrap_or_default();

        // NOTE(zk): translate solc ArtifactId to zksolc otherwise
        // we won't be able to find it in the zksolc output
        let Some(target) = input
            .artifact_ids()
            .map(|(id, _)| id)
            .find(|id| id.source == self.target.source && id.name == self.target.name)
        else {
            eyre::bail!("unable to find zk target artifact for linking");
        };
        let target = &target;

        let linker = self.get_zk_linker(script_config)?;

        let create2_deployer = DEFAULT_CREATE2_DEPLOYER_ZKSYNC;
        let maybe_create2_link_output = use_create2
            .then(|| {
                linker
                    .zk_link_with_create2(
                        known_libraries.clone(),
                        create2_deployer,
                        script_config.config.create2_library_salt,
                        target,
                    )
                    .ok()
            })
            .flatten();

        let libraries = if let Some(output) = maybe_create2_link_output {
            output.libraries
        } else {
            let output = linker.zk_link_with_nonce_or_address(
                known_libraries,
                script_config.evm_opts.sender,
                script_config.sender_nonce,
                [target],
            )?;

            output.libraries
        };

        let mut factory_deps = Default::default();
        let mut libs = Default::default();
        linker
            .zk_collect_dependencies(target, &mut libs, Some(&mut factory_deps))
            .expect("able to enumerate all deps");

        let linked_contracts = linker
            .zk_get_linked_artifacts(
                // only retrieve target and its deps
                factory_deps.into_iter().chain(libs.into_iter()).chain([target]),
                &libraries,
            )
            .context("retrieving all fully linked contracts")?;

        let newly_linked_dual_compiled_contracts = linked_contracts
            .iter()
            .flat_map(|(needle, zk)| {
                evm_linked_contracts
                    .iter()
                    .find(|(id, _)| id.source == needle.source && id.name == needle.name)
                    .map(|(_, evm)| (needle, zk, evm))
            })
            .filter(|(_, zk, evm)| zk.bytecode.is_some() && evm.bytecode.is_some())
            .map(|(id, linked_zk, evm)| {
                let (_, unlinked_zk_artifact) = input
                    .artifact_ids()
                    .find(|(contract_id, _)| contract_id == id)
                    .expect("unable to find original (pre-linking) artifact");

                let zk_bytecode =
                    linked_zk.get_bytecode_bytes().expect("no EraVM bytecode (or unlinked)");
                let zk_hash = hash_bytecode(&zk_bytecode);
                let evm_deployed = evm
                    .get_deployed_bytecode_bytes()
                    .expect("no EVM deployed bytecode (or unlinked)");
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
        dual_compiled_contracts.extend(new_contracts);

        // now that we have an updated list of DualCompiledContracts
        // retrieve all the factory deps for a given contracts and store them
        new_contracts_deps.into_iter().for_each(|(info, deps)| {
            deps.into_iter().for_each(|dep| {
                let mut split = dep.split(':');
                let path = split.next().expect("malformed factory dep path");
                let name = split.next().expect("malformed factory dep name");

                let bytecode = dual_compiled_contracts
                    .find(Some(path), Some(name))
                    .next()
                    .expect("unknown factory dep")
                    .1
                    .zk_deployed_bytecode
                    .clone();

                dual_compiled_contracts.insert_factory_deps(&info, Some(bytecode));
            });
        });

        self.dual_compiled_contracts.replace(dual_compiled_contracts);

        // base zksolc contracts + newly linked + evm contracts
        let contracts = input
            .artifact_ids()
            .map(|(id, v)| (id, CompactContractBytecode::from(CompactContractBytecodeCow::from(v))))
            .chain(linked_contracts)
            .chain(evm_linked_contracts)
            .collect();

        Ok(contracts)
    }
}
