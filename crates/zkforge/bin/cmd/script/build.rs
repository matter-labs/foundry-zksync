use crate::cmd::install;

use super::*;
use alloy_primitives::{Address, Bytes};
use eyre::{Context, ContextCompat, Result};

use foundry_cli::utils::LoadConfig;
use foundry_common::{
    compact_to_contract, compile::ContractSources, fs, zksolc_manager::setup_zksolc_manager,
};
use foundry_compilers::{
    artifacts::{CompactContractBytecode, ContractBytecode, ContractBytecodeSome, Libraries},
    contracts::ArtifactContracts,
    info::{ContractInfo, FullContractInfo},
    ArtifactId, Project, ProjectCompileOutput,
};

use std::{collections::BTreeMap, path::Path, str::FromStr};
use zkforge::link::{link_with_nonce_or_address, PostLinkInput, ResolvedDependency};

impl ScriptArgs {
    /// Compiles the file or project and the verify metadata.
    pub async fn compile(&mut self, script_config: &mut ScriptConfig) -> Result<BuildOutput> {
        trace!(target: "script", "compiling script");

        self.build(script_config).await
    }

    /// Compiles the file with auto-detection and compiler params.
    pub async fn build(&mut self, script_config: &mut ScriptConfig) -> Result<BuildOutput> {
        let (project, output) = self.get_project_and_output(script_config).await?;
        let output = output.with_stripped_file_prefixes(project.root());

        let mut sources: ContractSources = Default::default();

        let contracts = output
            .into_artifacts()
            .map(|(id, artifact)| -> Result<_> {
                // Sources are only required for the debugger, but it *might* mean that there's
                // something wrong with the build and/or artifacts.
                if let Some(source) = artifact.source_file() {
                    let path = source
                        .ast
                        .ok_or_else(|| eyre::eyre!("source from artifact has no AST"))?
                        .absolute_path;
                    let abs_path = project.root().join(path);
                    let source_code = fs::read_to_string(abs_path).wrap_err_with(|| {
                        format!("failed to read artifact source file for `{}`", id.identifier())
                    })?;
                    let contract = artifact.clone().into_contract_bytecode();
                    let source_contract = compact_to_contract(contract)?;
                    sources
                        .0
                        .entry(id.clone().name)
                        .or_default()
                        .insert(source.id, (source_code, source_contract));
                } else {
                    warn!(?id, "source not found");
                }
                Ok((id, artifact))
            })
            .collect::<Result<ArtifactContracts>>()?;

        let mut output = self.link(
            project,
            contracts,
            script_config.config.parsed_libraries()?,
            script_config.evm_opts.sender,
            script_config.sender_nonce,
            &script_config.config.script,
        )?;

        output.sources = sources;
        script_config.target_contract = Some(output.target.clone());

        Ok(output)
    }

    pub fn link<P: AsRef<Path>>(
        &self,
        project: Project,
        contracts: ArtifactContracts,
        libraries_addresses: Libraries,
        sender: Address,
        nonce: u64,
        script_path: P,
    ) -> Result<BuildOutput> {
        let mut run_dependencies = vec![];
        let mut contract = CompactContractBytecode::default();
        let mut highlevel_known_contracts = BTreeMap::new();

        //FIXME: remove - we temporarily parse this path:name (possibility)
        // since we don't handle name-only contract for now
        // otherwise self.path would be clean without the :name and we could
        // use that directly
        let contract_info = ContractInfo::from_str(&self.path)?;

        let mut target_fname =
            dunce::canonicalize(contract_info.path.wrap_err("unknown path for contract")?)
                .wrap_err("Couldn't convert contract path to absolute path.")?
                .strip_prefix(project.root())
                .wrap_err("Couldn't strip project root from contract path.")?
                .to_str()
                .wrap_err("Bad path to string.")?
                .to_string();

        let no_target_name = if let Some(target_name) = &self.target_contract {
            target_fname = target_fname + ":" + target_name;
            false
        } else {
            true
        };

        let mut extra_info = ExtraLinkingInfo {
            no_target_name,
            target_fname: target_fname.clone(),
            contract: &mut contract,
            dependencies: &mut run_dependencies,
            matched: false,
            target_id: None,
        };

        // link_with_nonce_or_address expects absolute paths
        let mut libs = libraries_addresses.clone();
        for (file, libraries) in libraries_addresses.libs.iter() {
            if file.is_relative() {
                let mut absolute_path = project.root().clone();
                absolute_path.push(file);
                libs.libs.insert(absolute_path, libraries.clone());
            }
        }

        link_with_nonce_or_address(
            contracts.clone(),
            &mut highlevel_known_contracts,
            libs,
            sender,
            nonce,
            &mut extra_info,
            |post_link_input| {
                let PostLinkInput {
                    contract,
                    known_contracts: highlevel_known_contracts,
                    id,
                    extra,
                    dependencies,
                } = post_link_input;

                fn unique_deps(deps: Vec<ResolvedDependency>) -> Vec<(String, Bytes)> {
                    let mut filtered = Vec::new();
                    let mut seen = HashSet::new();
                    for dep in deps {
                        if !seen.insert(dep.id.clone()) {
                            continue
                        }
                        filtered.push((dep.id, dep.bytecode));
                    }

                    filtered
                }

                // if it's the target contract, grab the info
                if extra.no_target_name {
                    // Match artifact source, and ignore interfaces
                    if id.source == std::path::Path::new(&extra.target_fname) &&
                        contract.bytecode.as_ref().map_or(false, |b| b.object.bytes_len() > 0)
                    {
                        if extra.matched {
                            eyre::bail!("Multiple contracts in the target path. Please specify the contract name with `--tc ContractName`")
                        }
                        *extra.dependencies = unique_deps(dependencies);
                        *extra.contract = contract.clone();
                        extra.matched = true;
                        extra.target_id = Some(id.clone());
                    }
                } else {
                    let FullContractInfo { path, name } =
                        FullContractInfo::from_str(&extra.target_fname)
                            .expect("The target specifier is malformed.");
                    let path = std::path::Path::new(&path);

                    // Make sure the path to script is absolute, since
                    // `script_path` is an absolute path
                    let path = path
                        .is_relative()
                        .then(|| project.root().join(path))
                        .unwrap_or_else(|| path.to_path_buf());

                    // Remove dir prefix for files in script dir
                    let new_path = path.strip_prefix(&script_path).unwrap_or(&path);

                    if new_path == id.source && name == id.name {
                        *extra.dependencies = unique_deps(dependencies);
                        *extra.contract = contract.clone();
                        extra.matched = true;
                        extra.target_id = Some(id.clone());
                    }
                }

                if let Ok(tc) = ContractBytecode::from(contract).try_into() {
                    highlevel_known_contracts.insert(id, tc);
                }

                Ok(())
            },
            project.root(),
        )?;

        let target = extra_info
            .target_id
            .ok_or_else(|| eyre::eyre!("Could not find target contract: {}", target_fname))?;

        let (new_libraries, predeploy_libraries): (Vec<_>, Vec<_>) =
            run_dependencies.into_iter().unzip();

        // Merge with user provided libraries
        let mut new_libraries = Libraries::parse(&new_libraries)?;
        for (file, libraries) in libraries_addresses.libs.into_iter() {
            new_libraries.libs.entry(file).or_default().extend(libraries)
        }

        Ok(BuildOutput {
            target,
            contract,
            known_contracts: contracts,
            highlevel_known_contracts: ArtifactContracts(highlevel_known_contracts),
            predeploy_libraries,
            sources: Default::default(),
            project,
            libraries: new_libraries,
        })
    }

    pub async fn get_project_and_output(
        &mut self,
        script_config: &ScriptConfig,
    ) -> Result<(Project, ProjectCompileOutput)> {
        let mut project = script_config.config.project()?;
        let mut zksolc_cfg = script_config.config.zk_solc_config().map_err(|e| eyre::eyre!(e))?;
        let mut config = script_config.config.clone();
        let contract = ContractInfo::from_str(&self.path)?;
        self.target_contract = Some(contract.name.clone());

        // A contract was specified by path
        // TODO: uncomment the `if let` block once we support script by contract name
        // if let Ok(_) = dunce::canonicalize(&self.path) {
        let compiler_path = setup_zksolc_manager(self.opts.args.use_zksolc.clone()).await?;
        zksolc_cfg.compiler_path = compiler_path;

        if install::install_missing_dependencies(&mut config, self.opts.args.silent) &&
            script_config.config.auto_detect_remappings
        {
            // need to re-configure here to also catch additional remappings
            config = self.opts.load_config();
            project = config.project()?;
            zksolc_cfg = config.zk_solc_config().map_err(|e| eyre::eyre!(e))?;
        }

        match foundry_common::zk_compile::compile_smart_contracts(zksolc_cfg, project) {
            Ok((project_compile_output, _contract_bytecodes)) => {
                Ok((config.project()?, project_compile_output))
            }
            Err(e) => eyre::bail!("Failed to compile with zksolc: {e}"),
        }
        //FIXME: add support for specifying contract name only in `script` invocations

        // // If we get here we received a contract name since the path wasn't valid.
        // // Attempt to retrieve the contract by name instead

        // ANNOTATED REFERENCE:
        // // Compile the contract (will populate cache with compiled artifact and provenance)
        // let output = if self.opts.args.silent {
        //     compile::suppress_compile(&project)
        // } else {
        //     compile::compile(&project, false, false)
        // }?;
        //
        // let cache =
        //     SolFilesCache::read_joined(&project.paths).wrap_err("Could not open compiler
        // cache")?;

        // // Lookup in cache the contrace by name, storing the now known path
        // let (path, _) = get_cached_entry_by_name(&cache, &contract.name)
        //     .wrap_err("Could not find target contract in cache")?;
        // self.path = path.to_string_lossy().to_string();
    }
}

struct ExtraLinkingInfo<'a> {
    no_target_name: bool,
    target_fname: String,
    contract: &'a mut CompactContractBytecode,
    dependencies: &'a mut Vec<(String, Bytes)>,
    matched: bool,
    target_id: Option<ArtifactId>,
}

pub struct BuildOutput {
    pub project: Project,
    pub target: ArtifactId,
    pub contract: CompactContractBytecode,
    pub known_contracts: ArtifactContracts,
    pub highlevel_known_contracts: ArtifactContracts<ContractBytecodeSome>,
    pub libraries: Libraries,
    pub predeploy_libraries: Vec<Bytes>,
    pub sources: ContractSources,
}
