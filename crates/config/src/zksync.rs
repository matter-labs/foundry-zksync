use foundry_compilers::{
    artifacts::{EvmVersion, Libraries, Severity},
    error::SolcError,
    solc::{CliSettings, Solc, SolcCompiler, SolcLanguage},
    Project, ProjectBuilder, ProjectPathsConfig,
};
use foundry_zksync_compilers::{
    artifacts::output_selection::{FileOutputSelection, OutputSelection, OutputSelectionFlag},
    compilers::{
        artifact_output::zk::ZkArtifactOutput,
        zksolc::{
            get_solc_version_info,
            settings::{
                BytecodeHash, Codegen, Optimizer, OptimizerDetails, SettingsMetadata,
                ZkSolcSettings,
            },
            ErrorType, WarningType, ZkSettings, ZkSolc, ZkSolcCompiler,
        },
    },
};
use semver::Version;
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::HashSet, path::PathBuf, str::FromStr};

use crate::{Config, SkipBuildFilters, SolcReq};

/// Filename for zksync cache
pub const ZKSYNC_SOLIDITY_FILES_CACHE_FILENAME: &str = "zksync-solidity-files-cache.json";

/// Directory for zksync artifacts
pub const ZKSYNC_ARTIFACTS_DIR: &str = "zkout";

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

    /// Hash type for the the metadata hash appended by zksolc to the compiled bytecode.
    pub hash_type: Option<BytecodeHash>,

    /// Hash type for the the metadata hash appended by zksolc to the compiled bytecode.
    /// Deprecated in favor of `hash_type`
    pub bytecode_hash: Option<BytecodeHash>,

    /// Whether to try to recompile with -Oz if the bytecode is too large.
    pub fallback_oz: bool,

    /// Whether to support compilation of zkSync-specific simulations
    pub enable_eravm_extensions: bool,

    /// Force evmla for zkSync
    pub force_evmla: bool,

    pub llvm_options: Vec<String>,

    /// Enable optimizer for zkSync
    pub optimizer: bool,

    /// The optimization mode string for zkSync
    pub optimizer_mode: char,

    /// zkSolc optimizer details
    pub optimizer_details: Option<OptimizerDetails>,

    // zksolc suppressed warnings.
    #[serde(deserialize_with = "deserialize_warning_set")]
    pub suppressed_warnings: HashSet<WarningType>,

    // zksolc suppressed errors.
    #[serde(deserialize_with = "deserialize_error_set")]
    pub suppressed_errors: HashSet<ErrorType>,
}

impl Default for ZkSyncConfig {
    fn default() -> Self {
        Self {
            compile: false,
            startup: false,
            zksolc: Default::default(),
            solc_path: Default::default(),
            hash_type: Default::default(),
            bytecode_hash: Default::default(),
            fallback_oz: Default::default(),
            enable_eravm_extensions: Default::default(),
            force_evmla: Default::default(),
            llvm_options: Default::default(),
            optimizer: true,
            optimizer_mode: '3',
            optimizer_details: Default::default(),
            suppressed_errors: Default::default(),
            suppressed_warnings: Default::default(),
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
        offline: bool,
    ) -> Result<ZkSolcSettings, SolcError> {
        let optimizer = Optimizer {
            enabled: Some(self.optimizer),
            mode: Some(self.optimizer_mode),
            fallback_to_optimizing_for_size: Some(self.fallback_oz),
            disable_system_request_memoization: Some(true),
            details: self.optimizer_details.clone(),
            jump_table_density_threshold: None,
        };

        let settings = ZkSettings {
            libraries,
            optimizer,
            evm_version: Some(evm_version),
            metadata: Some(SettingsMetadata::new(self.hash_type.or(self.bytecode_hash))),
            via_ir: Some(via_ir),
            // Set in project paths.
            remappings: Vec::new(),
            enable_eravm_extensions: self.enable_eravm_extensions,
            force_evmla: self.force_evmla,
            llvm_options: self.llvm_options.clone(),
            output_selection: OutputSelection {
                all: FileOutputSelection {
                    per_file: [].into(),
                    per_contract: [OutputSelectionFlag::ABI].into(),
                },
            },
            codegen: if self.force_evmla { Codegen::EVMLA } else { Codegen::Yul },
            suppressed_warnings: self.suppressed_warnings.clone(),
            suppressed_errors: self.suppressed_errors.clone(),
        };

        let zksolc_path = if let Some(path) = config_ensure_zksolc(self.zksolc.as_ref(), offline)? {
            path
        } else if !offline {
            let default_version = semver::Version::new(1, 5, 11);
            let mut zksolc = ZkSolc::find_installed_version(&default_version)?;
            if zksolc.is_none() {
                ZkSolc::blocking_install(&default_version)?;
                zksolc = ZkSolc::find_installed_version(&default_version)?;
            }
            zksolc.unwrap_or_else(|| panic!("Could not install zksolc v{default_version}"))
        } else {
            "zksolc".into()
        };

        // `cli_settings` get set from `Project` values when building `ZkSolcVersionedInput`
        ZkSolcSettings::new_from_path(settings, CliSettings::default(), zksolc_path)
    }
}

// Config overrides to create zksync specific foundry-compilers data structures

/// Returns the configured `zksolc` `Settings` that includes:
/// - all libraries
/// - the optimizer (including details, if configured)
/// - evm version
pub fn config_zksolc_settings(config: &Config) -> Result<ZkSolcSettings, SolcError> {
    let libraries = match config.parsed_libraries() {
        Ok(libs) => config.project_paths::<ProjectPathsConfig>().apply_lib_remappings(libs),
        Err(e) => return Err(SolcError::msg(format!("Failed to parse libraries: {e}"))),
    };

    config.zksync.settings(libraries, config.evm_version, config.via_ir, config.offline)
}

/// Create a new ZKsync project
pub fn config_create_project(
    config: &Config,
    cached: bool,
    no_artifacts: bool,
) -> Result<Project<ZkSolcCompiler, ZkArtifactOutput>, SolcError> {
    let mut builder = ProjectBuilder::<ZkSolcCompiler, ZkArtifactOutput>::default()
        .artifacts(ZkArtifactOutput {})
        .paths(config_project_paths(config))
        .settings(config_zksolc_settings(config)?)
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
        let filter = SkipBuildFilters::new(config.skip.clone(), config.root.clone());
        builder = builder.sparse_output(filter);
    }

    let zksolc_compiler = ZkSolcCompiler { solc: config_solc_compiler(config)? };

    let project = builder.build(zksolc_compiler)?;

    if config.force {
        config.cleanup(&project)?;
    }

    Ok(project)
}

/// Returns solc compiler to use along zksolc using the following rules:
/// 1. If `solc_path` in zksync config options is set, use it.
/// 2. If `solc_path` is not set, check the `solc` requirements: a. If a version is specified, use
///    zkVm solc matching that version. b. If a path is specified, use it.
/// 3. If none of the above, use autodetect which will match source files to a compiler version and
///    use zkVm solc matching that version.
fn config_solc_compiler(config: &Config) -> Result<SolcCompiler, SolcError> {
    if let Some(path) = &config.zksync.solc_path {
        if !path.is_file() {
            return Err(SolcError::msg(format!("`solc` {} does not exist", path.display())));
        }
        let version = get_solc_version_info(path)?.version;
        let solc =
            Solc::new_with_version(path, Version::new(version.major, version.minor, version.patch));
        return Ok(SolcCompiler::Specific(solc));
    }

    if let Some(ref solc) = config.solc {
        let solc = match solc {
            SolcReq::Version(version) => {
                let solc_version_without_metadata =
                    format!("{}.{}.{}", version.major, version.minor, version.patch);
                let maybe_solc =
                    ZkSolc::find_solc_installed_version(&solc_version_without_metadata)?;
                let path = if let Some(solc) = maybe_solc {
                    solc
                } else {
                    ZkSolc::solc_blocking_install(&solc_version_without_metadata)?
                };
                Solc::new_with_version(
                    path,
                    Version::new(version.major, version.minor, version.patch),
                )
            }
            SolcReq::Local(path) => {
                if !path.is_file() {
                    return Err(SolcError::msg(format!("`solc` {} does not exist", path.display())));
                }
                let version = get_solc_version_info(path)?.version;
                Solc::new_with_version(
                    path,
                    Version::new(version.major, version.minor, version.patch),
                )
            }
        };
        Ok(SolcCompiler::Specific(solc))
    } else {
        Ok(SolcCompiler::AutoDetect)
    }
}

/// Returns the `ProjectPathsConfig` sub set of the config.
pub fn config_project_paths(config: &Config) -> ProjectPathsConfig<SolcLanguage> {
    let builder = ProjectPathsConfig::builder()
        .cache(config.cache_path.join(ZKSYNC_SOLIDITY_FILES_CACHE_FILENAME))
        .sources(&config.src)
        .tests(&config.test)
        .scripts(&config.script)
        .artifacts(config.root.join(ZKSYNC_ARTIFACTS_DIR))
        .libs(config.libs.iter())
        .remappings(config.get_all_remappings())
        .allowed_path(&config.root)
        .allowed_paths(&config.libs)
        .allowed_paths(&config.allow_paths)
        .include_paths(&config.include_paths);

    builder.build_with_root(&config.root)
}

/// Ensures that the configured version is installed if explicitly set
///
/// If `zksolc` is [`SolcReq::Version`] then this will download and install the solc version if
/// it's missing, unless the `offline` flag is enabled, in which case an error is thrown.
///
/// If `zksolc` is [`SolcReq::Local`] then this will ensure that the path exists.
pub fn config_ensure_zksolc(
    zksolc: Option<&SolcReq>,
    offline: bool,
) -> Result<Option<PathBuf>, SolcError> {
    if let Some(ref zksolc) = zksolc {
        let zksolc = match zksolc {
            SolcReq::Version(version) => {
                let mut zksolc = ZkSolc::find_installed_version(version)?;
                if zksolc.is_none() {
                    if offline {
                        return Err(SolcError::msg(format!(
                            "can't install missing zksolc {version} in offline mode"
                        )));
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
                    )));
                }
                Some(zksolc.clone())
            }
        };
        return Ok(zksolc);
    }

    Ok(None)
}

fn deserialize_warning_set<'de, D>(deserializer: D) -> Result<HashSet<WarningType>, D::Error>
where
    D: Deserializer<'de>,
{
    let strings: Vec<String> = Vec::deserialize(deserializer)?;
    Ok(strings
        .into_iter()
        .filter_map(|s| match WarningType::from_str(&s) {
            Ok(warning) => Some(warning),
            Err(e) => {
                error!("Failed to parse warning type: '{}' with error: {}", s, e);
                None
            }
        })
        .collect())
}

fn deserialize_error_set<'de, D>(deserializer: D) -> Result<HashSet<ErrorType>, D::Error>
where
    D: Deserializer<'de>,
{
    let strings: Vec<String> = Vec::deserialize(deserializer)?;
    Ok(strings
        .into_iter()
        .filter_map(|s| match ErrorType::from_str(&s) {
            Ok(error) => Some(error),
            Err(e) => {
                error!("Failed to parse error type: '{}' with error: {}", s, e);
                None
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use foundry_compilers::solc::SolcCompiler;
    use semver::Version;

    use crate::Config;

    use super::*;

    #[test]
    fn zksync_project_has_zksync_solc_when_solc_req_is_a_version() {
        let config =
            Config { solc: Some(SolcReq::Version(Version::new(0, 8, 26))), ..Default::default() };
        let project = config_create_project(&config, false, true).unwrap();
        let solc_compiler = project.compiler.solc;
        if let SolcCompiler::Specific(path) = solc_compiler {
            let version = get_solc_version_info(&path.solc).unwrap();
            assert!(version.zksync_version.is_some());
        } else {
            panic!("Expected SolcCompiler::Specific");
        }
    }
}
