use crate::provider::VerificationContext;

use super::{VerifyArgs, VerifyCheckArgs};
use alloy_json_abi::JsonAbi;
use async_trait::async_trait;
use eyre::{OptionExt, Result};
use foundry_common::compile::ProjectCompiler;
use foundry_compilers::{
    artifacts::{output_selection::OutputSelection, Source},
    compilers::{solc::SolcCompiler, CompilerSettings},
    resolver::parse::SolData,
    solc::Solc,
    zksolc::ZkSolcCompiler,
    zksync::artifact_output::zk::ZkArtifactOutput,
    Graph, Project,
};
use foundry_config::Config;
use semver::Version;
use std::path::PathBuf;

/// Container with data required for contract verification.
#[derive(Debug, Clone)]
pub struct ZkVerificationContext {
    pub config: Config,
    pub project: Project<ZkSolcCompiler, ZkArtifactOutput>,
    pub target_path: PathBuf,
    pub target_name: String,
    pub compiler_version: Version,
}

impl ZkVerificationContext {
    pub fn new(
        target_path: PathBuf,
        target_name: String,
        compiler_version: Version,
        config: Config,
    ) -> Result<Self> {
        let mut project =
            foundry_zksync_compiler::config_create_project(&config, config.cache, false)?;
        project.no_artifacts = true;

        let solc = Solc::find_or_install(&compiler_version)?;
        project.compiler.solc = SolcCompiler::Specific(solc);

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
            .zksync_compile(&project, None)?;

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
            .zksync_compile(&project, None)?;

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

/// An abstraction for various verification providers such as etherscan, sourcify, blockscout
#[async_trait]
pub trait ZkVerificationProvider {
    /// This should ensure the verify request can be prepared successfully.
    ///
    /// Caution: Implementers must ensure that this _never_ sends the actual verify request
    /// `[VerificationProvider::verify]`, instead this is supposed to evaluate whether the given
    /// [`VerifyArgs`] are valid to begin with. This should prevent situations where there's a
    /// contract deployment that's executed before the verify request and the subsequent verify task
    /// fails due to misconfiguration.
    async fn preflight_check(
        &mut self,
        args: VerifyArgs,
        context: ZkVerificationContext,
    ) -> Result<()>;

    /// Sends the actual verify request for the targeted contract.
    async fn verify(&mut self, args: VerifyArgs, context: ZkVerificationContext) -> Result<()>;

    /// Checks whether the contract is verified.
    async fn check(&self, args: VerifyCheckArgs) -> Result<()>;
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
            Self::ZkSolc(c) => &c.compiler_version,
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
}
