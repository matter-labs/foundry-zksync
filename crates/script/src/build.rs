use crate::{
    broadcast::BundledState, execute::LinkedState, multi_sequence::MultiChainSequence,
    sequence::ScriptSequenceKind, ScriptArgs, ScriptConfig,
};
use alloy_primitives::{keccak256, Bytes, B256};
use alloy_provider::Provider;
use eyre::{Context, OptionExt, Result};
use forge_script_sequence::ScriptSequence;
use foundry_cheatcodes::Wallets;
use foundry_common::{
    compile::ProjectCompiler, provider::try_get_http_provider, ContractData, ContractsByArtifact,
};
use foundry_compilers::{
    artifacts::{BytecodeObject, CompactContractBytecode, CompactContractBytecodeCow, Libraries},
    compilers::{multi::MultiCompilerLanguage, Language},
    contracts::ArtifactContracts,
    info::ContractInfo,
    solc::SolcLanguage,
    utils::source_files_iter,
    Artifact, ArtifactId, ProjectCompileOutput,
};
use foundry_evm::traces::debug::ContractSources;
use foundry_linking::{Linker, ZkLinker};
use foundry_zksync_compilers::{
    compilers::{artifact_output::zk::ZkArtifactOutput, zksolc::ZkSolcCompiler},
    dual_compiled_contracts::{DualCompiledContract, DualCompiledContracts},
};
use foundry_zksync_core::{hash_bytecode, DEFAULT_CREATE2_DEPLOYER_ZKSYNC, H256};
use semver::VersionReq;
use std::{path::PathBuf, str::FromStr, sync::Arc};

/// Container for the compiled contracts.
#[derive(Debug)]
pub struct BuildData {
    /// Root of the project.
    pub project_root: PathBuf,
    /// The compiler output.
    pub output: ProjectCompileOutput,
    /// The zk compiler output
    pub zk_output: Option<ProjectCompileOutput<ZkSolcCompiler, ZkArtifactOutput>>,
    /// ID of target contract artifact.
    pub target: ArtifactId,
    pub dual_compiled_contracts: Option<DualCompiledContracts>,
}

impl BuildData {
    pub fn get_linker(&self) -> Linker<'_> {
        Linker::new(self.project_root.clone(), self.output.artifact_ids().collect())
    }

    fn get_zk_linker(&self, script_config: &ScriptConfig) -> Result<ZkLinker<'_>> {
        let zksolc = foundry_config::zksync::config_zksolc_compiler(&script_config.config)
            .context("retrieving zksolc compiler to be used for linking")?;

        let version_req = VersionReq::parse(">=1.5.8").unwrap();
        if !version_req.matches(&zksolc.version().context("trying to determine zksolc version")?) {
            eyre::bail!("linking requires zksolc >= 1.5.8");
        }

        let Some(input) = self.zk_output.as_ref() else {
            eyre::bail!("unable to link zk artifacts if no zk compilation output is provided")
        };

        Ok(ZkLinker::new(
            self.project_root.clone(),
            input
                .artifact_ids()
                .map(|(id, v)| (id.with_stripped_file_prefixes(self.project_root.as_ref()), v))
                .collect(),
            zksolc,
            input
        ))
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
    async fn zk_link(
        &mut self,
        script_config: &ScriptConfig,
        known_libraries: Libraries,
        evm_linked_artifacts: ArtifactContracts,
    ) -> Result<ArtifactContracts> {
        if !script_config.config.zksync.should_compile() {
            return Ok(evm_linked_artifacts);
        }

        let Some(input) = self.zk_output.as_ref() else {
            eyre::bail!("unable to link zk artifacts if no zk compilation output is provided");
        };
        let mut dual_compiled_contracts = self.dual_compiled_contracts.take().unwrap_or_default();

        let linker = self.get_zk_linker(script_config)?;

        // NOTE(zk): translate solc ArtifactId to zksolc otherwise
        // we won't be able to find it in the zksolc output
        let target = self.target.clone().with_stripped_file_prefixes(self.project_root.as_ref());
        let Some(target) = linker
            .linker
            .contracts
            .keys()
            .find(|id| id.source == target.source && id.name == target.name)
        else {
            eyre::bail!("unable to find zk target artifact for linking");
        };

        let create2_deployer = DEFAULT_CREATE2_DEPLOYER_ZKSYNC;
        let can_use_create2 = if let Some(fork_url) = &script_config.evm_opts.fork_url {
            let provider = try_get_http_provider(fork_url)?;
            let deployer_code = provider.get_code_at(create2_deployer).await?;

            !deployer_code.is_empty()
        } else {
            // If --fork-url is not provided, we are just simulating the script.
            true
        };

        let maybe_create2_link_output = can_use_create2
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

        let linked_contracts = linker
            .zk_get_linked_artifacts(
                linker
                    .linker
                    .contracts
                    .iter()
                    // we add this filter to avoid linking contracts that don't need linking
                    .filter(|(_, art)| {
                        // NOTE(zk): no need to check `deployed_bytecode`
                        // as those `link_references` would be the same as `bytecode`
                        art.bytecode
                            .as_ref()
                            .map(|bc| !bc.link_references.is_empty())
                            .unwrap_or_default()
                    })
                    .map(|(id, _)| id),
                &libraries,
            )
            .context("retrieving all fully linked contracts")?;

        let newly_linked_dual_compiled_contracts = linked_contracts
            .iter()
            .flat_map(|(needle, zk)| {
                evm_linked_artifacts
                    .iter()
                    .find(|(id, _)| id.source == needle.source && id.name == needle.name)
                    .map(|(_, evm)| (needle, zk, evm))
            })
            .filter(|(_, zk, evm)| zk.bytecode.is_some() && evm.bytecode.is_some())
            .map(|(id, linked_zk, evm)| {
                let (_, unlinked_zk_artifact) = input
                    .artifact_ids()
                    .find(|(contract_id, _)| {
                        contract_id.clone().with_stripped_file_prefixes(self.project_root.as_ref())
                            == id.clone()
                    })
                    .expect("unable to find original (pre-linking) artifact");
                let zk_bytecode =
                    linked_zk.get_bytecode_bytes().expect("no EraVM bytecode (or unlinked)");
                let zk_hash = hash_bytecode(&zk_bytecode);
                let evm = evm.get_bytecode_bytes().expect("no EVM bytecode (or unlinked)");
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
                dual_compiled_contracts.extend_factory_deps_by_hash(
                    contract,
                    unlinked_zk_artifact.factory_dependencies.iter().flatten().map(|(hash, _)| {
                        H256::from_slice(
                            alloy_primitives::hex::decode(dbg!(hash))
                                .expect("malformed factory dep hash")
                                .as_slice(),
                        )
                    }),
                )
            });

        dual_compiled_contracts.extend(newly_linked_dual_compiled_contracts.collect::<Vec<_>>());
        self.dual_compiled_contracts.replace(dual_compiled_contracts);

        // base zksolc contracts + newly linked + evm contracts
        let contracts = input
            .artifact_ids()
            .map(|(id, v)| {
                (
                    id.with_stripped_file_prefixes(self.project_root.as_ref()),
                    CompactContractBytecode::from(CompactContractBytecodeCow::from(v)),
                )
            })
            .chain(linked_contracts)
            .chain(evm_linked_artifacts)
            .collect();

        Ok(contracts)
    }

    /// Links contracts. Uses CREATE2 linking when possible, otherwise falls back to
    /// default linking with sender nonce and address.
    pub async fn link(mut self, script_config: &ScriptConfig) -> Result<LinkedBuildData> {
        let create2_deployer = script_config.evm_opts.create2_deployer;
        let can_use_create2 = if let Some(fork_url) = &script_config.evm_opts.fork_url {
            let provider = try_get_http_provider(fork_url)?;
            let deployer_code = provider.get_code_at(create2_deployer).await?;

            !deployer_code.is_empty()
        } else {
            // If --fork-url is not provided, we are just simulating the script.
            true
        };

        let known_libraries = script_config.config.libraries_with_remappings()?;

        // TODO(zk): evaluate using strategies here as well

        let maybe_create2_link_output = can_use_create2
            .then(|| {
                self.get_linker()
                    .link_with_create2(
                        known_libraries.clone(),
                        create2_deployer,
                        script_config.config.create2_library_salt,
                        &self.target,
                    )
                    .ok()
            })
            .flatten();

        let (libraries, predeploy_libs) = if let Some(output) = maybe_create2_link_output {
            (
                output.libraries,
                ScriptPredeployLibraries::Create2(
                    output.libs_to_deploy,
                    script_config.config.create2_library_salt,
                ),
            )
        } else {
            let output = self.get_linker().link_with_nonce_or_address(
                known_libraries.clone(),
                script_config.evm_opts.sender,
                script_config.sender_nonce,
                [&self.target],
            )?;

            (output.libraries, ScriptPredeployLibraries::Default(output.libs_to_deploy))
        };

        let known_contracts = self
            .get_linker()
            .get_linked_artifacts(&libraries)
            .context("retrieving fully linked artifacts")?;
        let known_contracts = self.zk_link(script_config, known_libraries, known_contracts).await?;

        LinkedBuildData::new(
            libraries,
            predeploy_libs,
            ContractsByArtifact::new(known_contracts),
            self,
        )
    }

    /// Links the build data with the given libraries. Expects supplied libraries set being enough
    /// to fully link target contract.
    pub async fn link_with_libraries(
        mut self,
        script_config: &ScriptConfig,
        libraries: Libraries,
    ) -> Result<LinkedBuildData> {
        let known_contracts = self.get_linker().get_linked_artifacts(&libraries)?;
        let known_contracts =
            self.zk_link(script_config, libraries.clone(), known_contracts).await?;

        LinkedBuildData::new(
            libraries,
            ScriptPredeployLibraries::Default(Vec::new()),
            ContractsByArtifact::new(known_contracts),
            self,
        )
    }
}

#[derive(Debug)]
pub enum ScriptPredeployLibraries {
    Default(Vec<Bytes>),
    Create2(Vec<Bytes>, B256),
}

impl ScriptPredeployLibraries {
    pub fn libraries_count(&self) -> usize {
        match self {
            Self::Default(libs) => libs.len(),
            Self::Create2(libs, _) => libs.len(),
        }
    }
}

/// Container for the linked contracts and their dependencies
#[derive(Debug)]
pub struct LinkedBuildData {
    /// Original build data, might be used to relink this object with different libraries.
    pub build_data: BuildData,
    /// Known fully linked contracts.
    pub known_contracts: ContractsByArtifact,
    /// Libraries used to link the contracts.
    pub libraries: Libraries,
    /// Libraries that need to be deployed by sender before script execution.
    pub predeploy_libraries: ScriptPredeployLibraries,
    /// Source files of the contracts. Used by debugger.
    pub sources: ContractSources,
}

impl LinkedBuildData {
    pub fn new(
        libraries: Libraries,
        predeploy_libraries: ScriptPredeployLibraries,
        known_contracts: ContractsByArtifact,
        build_data: BuildData,
    ) -> Result<Self> {
        let sources = ContractSources::from_project_output(
            &build_data.output,
            &build_data.project_root,
            Some(&libraries),
        )?;

        Ok(Self { build_data, known_contracts, libraries, predeploy_libraries, sources })
    }

    /// Fetches target bytecode from linked contracts.
    pub fn get_target_contract(&self) -> Result<&ContractData> {
        self.known_contracts
            .get(&self.build_data.target)
            .ok_or_eyre("target not found in linked artifacts")
    }
}

/// First state basically containing only inputs of the user.
pub struct PreprocessedState {
    pub args: ScriptArgs,
    pub script_config: ScriptConfig,
    pub script_wallets: Wallets,
}

impl PreprocessedState {
    /// Parses user input and compiles the contracts depending on script target.
    /// After compilation, finds exact [ArtifactId] of the target contract.
    pub fn compile(self) -> Result<CompiledState> {
        let Self { args, script_config, script_wallets } = self;
        let project = script_config.config.project()?;

        let mut target_name = args.target_contract.clone();

        // If we've received correct path, use it as target_path
        // Otherwise, parse input as <path>:<name> and use the path from the contract info, if
        // present.
        let target_path = if let Ok(path) = dunce::canonicalize(&args.path) {
            path
        } else {
            let contract = ContractInfo::from_str(&args.path)?;
            target_name = Some(contract.name.clone());
            if let Some(path) = contract.path {
                dunce::canonicalize(path)?
            } else {
                project.find_contract_path(contract.name.as_str())?
            }
        };

        #[allow(clippy::redundant_clone)]
        let sources_to_compile = source_files_iter(
            project.paths.sources.as_path(),
            MultiCompilerLanguage::FILE_EXTENSIONS,
        )
        .chain([target_path.to_path_buf()]);

        let output = ProjectCompiler::new().files(sources_to_compile).compile(&project)?;

        let mut zk_output = None;
        // ZK
        let dual_compiled_contracts = if script_config.config.zksync.should_compile() {
            let zk_project = foundry_config::zksync::config_create_project(
                &script_config.config,
                script_config.config.cache,
                false,
            )?;
            let sources_to_compile =
                source_files_iter(project.paths.sources.as_path(), SolcLanguage::FILE_EXTENSIONS)
                    .chain([target_path.clone()]);

            let zk_compiler = ProjectCompiler::new().files(sources_to_compile);

            zk_output = Some(zk_compiler.zksync_compile(&zk_project)?);
            Some(DualCompiledContracts::new(
                &output,
                &zk_output.clone().unwrap(),
                &project.paths,
                &zk_project.paths,
            ))
        } else {
            None
        };

        let mut target_id: Option<ArtifactId> = None;

        // Find target artfifact id by name and path in compilation artifacts.
        for (id, contract) in output.artifact_ids().filter(|(id, _)| id.source == target_path) {
            if let Some(name) = &target_name {
                if id.name != *name {
                    continue;
                }
            } else if contract.abi.as_ref().is_none_or(|abi| abi.is_empty())
                || contract.bytecode.as_ref().is_none_or(|b| match &b.object {
                    BytecodeObject::Bytecode(b) => b.is_empty(),
                    BytecodeObject::Unlinked(_) => false,
                })
            {
                // Ignore contracts with empty abi or linked bytecode of length 0 which are
                // interfaces/abstract contracts/libraries.
                continue;
            }

            if let Some(target) = target_id {
                // We might have multiple artifacts for the same contract but with different
                // solc versions. Their names will have form of {name}.0.X.Y, so we are
                // stripping versions off before comparing them.
                let target_name = target.name.split('.').next().unwrap();
                let id_name = id.name.split('.').next().unwrap();
                if target_name != id_name {
                    eyre::bail!("Multiple contracts in the target path. Please specify the contract name with `--tc ContractName`")
                }
            }
            target_id = Some(id);
        }

        let target = target_id.ok_or_eyre("Could not find target contract")?;

        Ok(CompiledState {
            args,
            script_config,
            script_wallets,
            build_data: BuildData {
                output,
                zk_output,
                target,
                project_root: project.root().clone(),
                dual_compiled_contracts,
            },
        })
    }
}

/// State after we have determined and compiled target contract to be executed.
pub struct CompiledState {
    pub args: ScriptArgs,
    pub script_config: ScriptConfig,
    pub script_wallets: Wallets,
    pub build_data: BuildData,
}

impl CompiledState {
    /// Uses provided sender address to compute library addresses and link contracts with them.
    pub async fn link(self) -> Result<LinkedState> {
        let Self { args, script_config, script_wallets, build_data } = self;

        let build_data = build_data.link(&script_config).await?;

        Ok(LinkedState { args, script_config, script_wallets, build_data })
    }

    /// Tries loading the resumed state from the cache files, skipping simulation stage.
    pub async fn resume(self) -> Result<BundledState> {
        let chain = if self.args.multi {
            None
        } else {
            let fork_url = self.script_config.evm_opts.fork_url.clone().ok_or_eyre("Missing --fork-url field, if you were trying to broadcast a multi-chain sequence, please use --multi flag")?;
            let provider = Arc::new(try_get_http_provider(fork_url)?);
            Some(provider.get_chain_id().await?)
        };

        let sequence = match self.try_load_sequence(chain, false) {
            Ok(sequence) => sequence,
            Err(_) => {
                // If the script was simulated, but there was no attempt to broadcast yet,
                // try to read the script sequence from the `dry-run/` folder
                let mut sequence = self.try_load_sequence(chain, true)?;

                // If sequence was in /dry-run, Update its paths so it is not saved into /dry-run
                // this time as we are about to broadcast it.
                sequence.update_paths_to_broadcasted(
                    &self.script_config.config,
                    &self.args.sig,
                    &self.build_data.target,
                )?;

                sequence.save(true, true)?;
                sequence
            }
        };

        let (args, build_data, script_wallets, script_config) = if !self.args.unlocked {
            let mut froms = sequence.sequences().iter().flat_map(|s| {
                s.transactions
                    .iter()
                    .skip(s.receipts.len())
                    .map(|t| t.transaction.from().expect("from is missing in script artifact"))
            });

            let available_signers = self
                .script_wallets
                .signers()
                .map_err(|e| eyre::eyre!("Failed to get available signers: {}", e))?;

            if !froms.all(|from| available_signers.contains(&from)) {
                // IF we are missing required signers, execute script as we might need to collect
                // private keys from the execution.
                let executed = self.link().await?.prepare_execution().await?.execute().await?;
                (
                    executed.args,
                    executed.build_data.build_data,
                    executed.script_wallets,
                    executed.script_config,
                )
            } else {
                (self.args, self.build_data, self.script_wallets, self.script_config)
            }
        } else {
            (self.args, self.build_data, self.script_wallets, self.script_config)
        };

        // Collect libraries from sequence and link contracts with them.
        let libraries = match sequence {
            ScriptSequenceKind::Single(ref seq) => Libraries::parse(&seq.libraries)?,
            // Library linking is not supported for multi-chain sequences
            ScriptSequenceKind::Multi(_) => Libraries::default(),
        };

        let linked_build_data = build_data.link_with_libraries(&script_config, libraries).await?;

        Ok(BundledState {
            args,
            script_config,
            script_wallets,
            build_data: linked_build_data,
            sequence,
        })
    }

    fn try_load_sequence(&self, chain: Option<u64>, dry_run: bool) -> Result<ScriptSequenceKind> {
        if let Some(chain) = chain {
            let sequence = ScriptSequence::load(
                &self.script_config.config,
                &self.args.sig,
                &self.build_data.target,
                chain,
                dry_run,
            )?;
            Ok(ScriptSequenceKind::Single(sequence))
        } else {
            let sequence = MultiChainSequence::load(
                &self.script_config.config,
                &self.args.sig,
                &self.build_data.target,
                dry_run,
            )?;
            Ok(ScriptSequenceKind::Multi(sequence))
        }
    }
}
