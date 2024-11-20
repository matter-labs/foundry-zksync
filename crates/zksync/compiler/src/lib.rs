//! # foundry-zksync
//!
//! Main Foundry ZKSync implementation.
#![warn(missing_docs, unused_crate_dependencies)]

/// ZKSolc specific logic.
mod zksolc;

use std::path::PathBuf;

use foundry_config::{Config, SkipBuildFilters, SolcReq};
use semver::Version;
pub use zksolc::*;

pub mod libraries;

use foundry_compilers::{
    artifacts::Severity,
    error::SolcError,
    solc::{Solc, SolcCompiler, SolcLanguage},
    zksolc::{get_solc_version_info, ZkSolc, ZkSolcCompiler, ZkSolcSettings},
    zksync::artifact_output::zk::ZkArtifactOutput,
    Project, ProjectBuilder, ProjectPathsConfig,
};

/// Filename for zksync cache
pub const ZKSYNC_SOLIDITY_FILES_CACHE_FILENAME: &str = "zksync-solidity-files-cache.json";

/// Directory for zksync artifacts
pub const ZKSYNC_ARTIFACTS_DIR: &str = "zkout";

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

    Ok(config.zksync.settings(libraries, config.evm_version, config.via_ir))
}

/// Create a new zkSync project
pub fn config_create_project(
    config: &Config,
    cached: bool,
    no_artifacts: bool,
) -> Result<Project<ZkSolcCompiler, ZkArtifactOutput>, SolcError> {
    let mut builder = ProjectBuilder::<ZkSolcCompiler>::default()
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
        let filter = SkipBuildFilters::new(config.skip.clone(), config.root.0.clone());
        builder = builder.sparse_output(filter);
    }

    let zksolc = if let Some(zksolc) =
        config_ensure_zksolc(config.zksync.zksolc.as_ref(), config.offline)?
    {
        zksolc
    } else if !config.offline {
        let default_version = semver::Version::new(1, 5, 7);
        let mut zksolc = ZkSolc::find_installed_version(&default_version)?;
        if zksolc.is_none() {
            ZkSolc::blocking_install(&default_version)?;
            zksolc = ZkSolc::find_installed_version(&default_version)?;
        }
        zksolc.unwrap_or_else(|| panic!("Could not install zksolc v{}", default_version))
    } else {
        "zksolc".into()
    };

    let zksolc_compiler = ZkSolcCompiler { zksolc, solc: config_solc_compiler(config)? };

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
            return Err(SolcError::msg(format!("`solc` {} does not exist", path.display())))
        }
        let version = get_solc_version_info(path)?.version;
        let solc =
            Solc::new_with_version(path, Version::new(version.major, version.minor, version.patch));
        return Ok(SolcCompiler::Specific(solc))
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
                    return Err(SolcError::msg(format!("`solc` {} does not exist", path.display())))
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
        .artifacts(config.root.0.join(ZKSYNC_ARTIFACTS_DIR))
        .libs(config.libs.iter())
        .remappings(config.get_all_remappings())
        .allowed_path(&config.root.0)
        .allowed_paths(&config.libs)
        .allowed_paths(&config.allow_paths)
        .include_paths(&config.include_paths);

    builder.build_with_root(&config.root.0)
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
                Some(zksolc.clone())
            }
        };
        return Ok(zksolc)
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
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
