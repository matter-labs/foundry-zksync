use foundry_compilers::{
    artifacts::{EvmVersion, Libraries, Severity},
    error::SolcError,
    solc::SolcCompiler,
    zksolc::{
        settings::{BytecodeHash, Optimizer, OptimizerDetails, SettingsMetadata, ZkSolcSettings},
        ZkSolc,
    },
    zksync::config::ZkSolcConfig,
    Project, ProjectBuilder,
};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{Config, SkipBuildFilters, SolcReq};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// ZkSync configuration
pub struct ZkSyncConfig {
    /// Compile for zkVM
    pub compile: bool,

    /// Start VM in zkVM mode
    pub startup: bool,

    /// The zkSolc instance to use if any.
    pub zksolc: Option<SolcReq>,

    /// solc path to use along the zksolc compiler
    pub solc_path: Option<PathBuf>,

    /// Whether to include the metadata hash for zksolc compiled bytecode.
    pub bytecode_hash: BytecodeHash,

    /// Whether to try to recompile with -Oz if the bytecode is too large.
    pub fallback_oz: bool,

    /// Whether to support compilation of zkSync-specific simulations
    pub eravm_extensions: bool,

    /// Force evmla for zkSync
    pub force_evmla: bool,

    /// Detect missing libraries, instead of erroring
    ///
    /// Currently unused
    pub detect_missing_libraries: bool,

    /// Source files to avoid compiling on zksolc
    pub avoid_contracts: Option<Vec<String>>,

    /// Enable optimizer for zkSync
    pub optimizer: bool,

    /// The optimization mode string for zkSync
    pub optimizer_mode: char,

    /// zkSolc optimizer details
    pub optimizer_details: Option<OptimizerDetails>,
}

impl Default for ZkSyncConfig {
    fn default() -> Self {
        Self {
            compile: Default::default(),
            startup: true,
            zksolc: Default::default(),
            solc_path: Default::default(),
            bytecode_hash: Default::default(),
            fallback_oz: Default::default(),
            eravm_extensions: Default::default(),
            force_evmla: Default::default(),
            detect_missing_libraries: Default::default(),
            avoid_contracts: Default::default(),
            optimizer: true,
            optimizer_mode: '3',
            optimizer_details: Default::default(),
        }
    }
}

impl ZkSyncConfig {
    /// Returns true if zk mode is enabled and it if tests should be run in zk mode
    pub fn run_in_zk_mode(&self) -> bool {
        self.compile && self.startup
    }

    /// Returns true if contracts should be compiled for zk
    pub fn should_compile(&self) -> bool {
        self.compile
    }

    /// Convert the zksync config to a foundry_compilers zksync Settings
    pub fn settings(
        &self,
        libraries: Libraries,
        evm_version: EvmVersion,
        via_ir: bool,
    ) -> ZkSolcSettings {
        let optimizer = Optimizer {
            enabled: Some(self.optimizer),
            mode: Some(self.optimizer_mode),
            fallback_to_optimizing_for_size: Some(self.fallback_oz),
            disable_system_request_memoization: Some(true),
            details: self.optimizer_details.clone(),
            jump_table_density_threshold: None,
        };

        ZkSolcSettings {
            libraries,
            optimizer,
            evm_version: Some(evm_version),
            metadata: Some(SettingsMetadata { bytecode_hash: Some(self.bytecode_hash) }),
            via_ir: Some(via_ir),
            // Set in project paths.
            remappings: Vec::new(),
            detect_missing_libraries: self.detect_missing_libraries,
            system_mode: self.eravm_extensions,
            force_evmla: self.force_evmla,
            // TODO: See if we need to set this from here
            output_selection: Default::default(),
            solc: self.solc_path.clone(),
        }
    }

    pub fn avoid_contracts(&self) -> Option<Vec<globset::GlobMatcher>> {
        self.avoid_contracts.clone().map(|patterns| {
            patterns
                .into_iter()
                .map(|pat| globset::Glob::new(&pat).expect("invalid pattern").compile_matcher())
                .collect::<Vec<_>>()
        })
    }
}

/// Ensures that the configured version is installed if explicitly set
///
/// If `zksolc` is [`SolcReq::Version`] then this will download and install the solc version if
/// it's missing, unless the `offline` flag is enabled, in which case an error is thrown.
///
/// If `zksolc` is [`SolcReq::Local`] then this will ensure that the path exists.
pub fn ensure_zksolc(zksolc: Option<&SolcReq>, offline: bool) -> Result<Option<ZkSolc>, SolcError> {
    if let Some(ref zksolc) = zksolc {
        let zksolc = match zksolc {
            SolcReq::Version(version) => {
                let mut zksolc = ZkSolc::find_installed_version(version)?;
                if zksolc.is_none() {
                    if offline {
                        return Err(SolcError::msg(format!(
                            "can't install missing zksolc {version} in offline mode"
                        )))
                    }
                    ZkSolc::blocking_install(version)?;
                    zksolc = ZkSolc::find_installed_version(version)?;
                }
                zksolc
            }
            SolcReq::Local(zksolc) => {
                if !zksolc.is_file() {
                    return Err(SolcError::msg(format!(
                        "`zksolc` {} does not exist",
                        zksolc.display()
                    )))
                }
                Some(ZkSolc::new(zksolc))
            }
        };
        return Ok(zksolc)
    }

    Ok(None)
}

/// Create a new zkSync project
pub fn create_project(
    config: &Config,
    cached: bool,
    no_artifacts: bool,
) -> Result<Project<SolcCompiler>, SolcError> {
    let mut builder = ProjectBuilder::<SolcCompiler>::default()
        .artifacts(config.configured_artifacts_handler())
        .paths(config.project_paths())
        .settings(config.solc_settings()?)
        .ignore_error_codes(config.ignored_error_codes.iter().copied().map(Into::into))
        .ignore_paths(config.ignored_file_paths.clone())
        .set_compiler_severity_filter(if config.deny_warnings {
            Severity::Warning
        } else {
            Severity::Error
        })
        .set_offline(config.offline)
        .set_cached(cached)
        .set_build_info(!no_artifacts && config.build_info)
        .set_no_artifacts(no_artifacts);

    if !config.skip.is_empty() {
        let filter = SkipBuildFilters::new(config.skip.clone(), config.root.0.clone());
        builder = builder.sparse_output(filter);
    }

    let mut project = builder.build(config.solc_compiler()?)?;

    if config.force {
        config.cleanup(&project)?;
    }

    // Set up zksolc project values
    // TODO: maybe some of these could be included
    // when setting up the builder for the sake of consistency (requires dedicated
    // builder methods)
    project.zksync_zksolc_config = ZkSolcConfig { settings: config.zksync_zksolc_settings()? };

    if let Some(zksolc) = ensure_zksolc(config.zksync.zksolc.as_ref(), config.offline)? {
        project.zksync_zksolc = zksolc;
    } else {
        // TODO: we automatically install a zksolc version
        // if none is found, but maybe we should mirror auto detect settings
        // as done with solc
        if !config.offline {
            let default_version = semver::Version::new(1, 5, 0);
            let mut zksolc = ZkSolc::find_installed_version(&default_version)?;
            if zksolc.is_none() {
                ZkSolc::blocking_install(&default_version)?;
                zksolc = ZkSolc::find_installed_version(&default_version)?;
            }
            project.zksync_zksolc =
                zksolc.unwrap_or_else(|| panic!("Could not install zksolc v{}", default_version));
        }
    }

    Ok(project)
}
