use crate::provider::VerificationContext;

use alloy_json_abi::JsonAbi;
use eyre::{eyre, OptionExt, Result};
use foundry_common::compile::ProjectCompiler;
use foundry_compilers::{
    artifacts::{output_selection::OutputSelection, BytecodeObject, Source},
    compilers::CompilerSettings,
    resolver::parse::SolData,
    solc::{Solc, SolcCompiler},
    Artifact, Graph, Project,
};
use foundry_config::Config;
use foundry_zksync_compilers::compilers::{
    artifact_output::zk::ZkArtifactOutput,
    zksolc::{self, ZkSolc, ZkSolcCompiler},
};
use revm_primitives::Bytes;
use semver::Version;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ZkVersion {
    pub zksolc: Version,
    pub solc: Version,
    pub is_zksync_solc: bool,
}

/// Container with data required for contract verification.
#[derive(Debug, Clone)]
pub struct ZkVerificationContext {
    pub config: Config,
    pub project: Project<ZkSolcCompiler, ZkArtifactOutput>,
    pub target_path: PathBuf,
    pub target_name: String,
    pub compiler_version: ZkVersion,
}

impl ZkVerificationContext {
    pub fn new(
        target_path: PathBuf,
        target_name: String,
        context_solc_version: Version,
        config: Config,
    ) -> Result<Self> {
        let mut project =
            foundry_config::zksync::config_create_project(&config, config.cache, false)?;
        project.no_artifacts = true;
        let zksolc_version = project.settings.zksolc_version_ref();

        let (solc_version, is_zksync_solc) = if let Some(solc) = &config.zksync.solc_path {
            let solc_type_and_version = zksolc::get_solc_version_info(solc)?;
            (solc_type_and_version.version, solc_type_and_version.zksync_version.is_some())
        } else {
            //if there's no `solc_path` specified then we use the same
            // as the project version
            let maybe_solc_path =
                ZkSolc::find_solc_installed_version(&context_solc_version.to_string())?;
            let solc_path = if let Some(p) = maybe_solc_path {
                p
            } else {
                ZkSolc::solc_blocking_install(&context_solc_version.to_string())?
            };

            let solc = Solc::new_with_version(solc_path, context_solc_version.clone());
            project.compiler.solc = SolcCompiler::Specific(solc);

            (context_solc_version, true)
        };

        let compiler_version =
            ZkVersion { zksolc: zksolc_version.clone(), solc: solc_version, is_zksync_solc };

        Ok(Self { config, project, target_name, target_path, compiler_version })
    }

    /// Compiles target contract requesting only ABI and returns it.
    pub fn get_target_abi(&self) -> Result<JsonAbi> {
        let mut project = self.project.clone();
        project.settings.update_output_selection(|selection| {
            *selection = OutputSelection::common_output_selection(["abi".to_string()])
        });

        let output = ProjectCompiler::new()
            .quiet(true)
            .files([self.target_path.clone()])
            .zksync_compile(&project)?;

        let artifact = output
            .find(&self.target_path, &self.target_name)
            .ok_or_eyre("failed to find target artifact when compiling for abi")?;

        artifact.abi.clone().ok_or_eyre("target artifact does not have an ABI")
    }

    /// Compiles target file requesting only metadata and returns it.
    pub fn get_target_metadata(&self) -> Result<serde_json::Value> {
        let mut project = self.project.clone();
        project.settings.update_output_selection(|selection| {
            *selection = OutputSelection::common_output_selection(["metadata".to_string()]);
        });

        let output = ProjectCompiler::new()
            .quiet(true)
            .files([self.target_path.clone()])
            .zksync_compile(&project)?;

        let artifact = output
            .find(&self.target_path, &self.target_name)
            .ok_or_eyre("failed to find target artifact when compiling for metadata")?;

        artifact.metadata.clone().ok_or_eyre("target artifact does not have an ABI")
    }

    /// Returns [Vec] containing imports of the target file.
    pub fn get_target_imports(&self) -> Result<Vec<PathBuf>> {
        let mut sources = self.project.paths.read_input_files()?;
        sources.insert(self.target_path.clone(), Source::read(&self.target_path)?);
        let graph = Graph::<SolData>::resolve_sources(&self.project.paths, sources)?;

        Ok(graph.imports(&self.target_path).into_iter().cloned().collect())
    }
}

#[derive(Debug)]
pub enum CompilerVerificationContext {
    Solc(VerificationContext),
    ZkSolc(ZkVerificationContext),
}

impl CompilerVerificationContext {
    pub fn config(&self) -> &Config {
        match self {
            Self::Solc(c) => &c.config,
            Self::ZkSolc(c) => &c.config,
        }
    }

    pub fn target_path(&self) -> &PathBuf {
        match self {
            Self::Solc(c) => &c.target_path,
            Self::ZkSolc(c) => &c.target_path,
        }
    }

    pub fn target_name(&self) -> &str {
        match self {
            Self::Solc(c) => &c.target_name,
            Self::ZkSolc(c) => &c.target_name,
        }
    }

    pub fn compiler_version(&self) -> &Version {
        match self {
            Self::Solc(c) => &c.compiler_version,
            // TODO: will refer to the solc version here. Analyze if we can remove
            // this ambiguity somehow (e.g: by having sepparate paths for solc/zksolc
            // and remove this method altogether)
            Self::ZkSolc(c) => &c.compiler_version.solc,
        }
    }

    pub fn get_target_abi(&self) -> Result<JsonAbi> {
        match self {
            Self::Solc(c) => c.get_target_abi(),
            Self::ZkSolc(c) => c.get_target_abi(),
        }
    }

    pub fn get_target_imports(&self) -> Result<Vec<PathBuf>> {
        match self {
            Self::Solc(c) => c.get_target_imports(),
            Self::ZkSolc(c) => c.get_target_imports(),
        }
    }

    pub fn get_target_metadata(&self) -> Result<serde_json::Value> {
        match self {
            Self::Solc(c) => {
                let m = c.get_target_metadata()?;
                Ok(serde_json::to_value(m)?)
            }
            Self::ZkSolc(c) => c.get_target_metadata(),
        }
    }

    pub fn get_target_bytecode(&self) -> Result<Bytes> {
        match self {
            Self::Solc(context) => {
                let output = context.project.compile_file(&context.target_path)?;
                let artifact = output
                    .find(&context.target_path, &context.target_name)
                    .ok_or_eyre("Contract artifact wasn't found locally")?;

                let bytecode = artifact
                    .get_bytecode_object()
                    .ok_or_eyre("Contract artifact does not contain bytecode")?;

                match bytecode.as_ref() {
                    BytecodeObject::Bytecode(bytes) => Ok(bytes.clone()),
                    BytecodeObject::Unlinked(_) => Err(eyre!(
                        "You have to provide correct libraries to use --guess-constructor-args"
                    )),
                }
            }
            Self::ZkSolc(context) => {
                let output = context.project.compile_file(&context.target_path)?;
                let artifact = output
                    .find(&context.target_path, &context.target_name)
                    .ok_or_eyre("Contract artifact wasn't found locally")?;

                let bytecode = artifact
                    .get_bytecode_object()
                    .ok_or_eyre("Contract artifact does not contain bytecode")?;

                match bytecode.as_ref() {
                    BytecodeObject::Bytecode(bytes) => Ok(bytes.clone()),
                    BytecodeObject::Unlinked(_) => Err(eyre!(
                        "You have to provide correct libraries to use --guess-constructor-args"
                    )),
                }
            }
        }
    }
}
