//! # foundry-zksync
//!
//! Main Foundry ZKSync implementation.
#![warn(missing_docs, unused_crate_dependencies)]

/// ZKSolc specific logic.
mod zksolc;

use std::path::PathBuf;

use foundry_config::{Config, SkipBuildFilters, SolcReq};
pub use zksolc::*;

pub mod libraries;

use foundry_compilers::{
    artifacts::Severity,
    error::SolcError,
    solc::SolcLanguage,
    zksolc::{ZkSolc, ZkSolcCompiler, ZkSolcSettings},
    zksync::artifact_output::zk::ZkArtifactOutput,
    Project, ProjectBuilder, ProjectPathsConfig,
};

/// Filename for zksync cache
pub const ZKSYNC_SOLIDITY_FILES_CACHE_FILENAME: &str = "zksync-solidity-files-cache.json";

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
        let default_version = semver::Version::new(1, 5, 1);
        let mut zksolc = ZkSolc::find_installed_version(&default_version)?;
        if zksolc.is_none() {
            ZkSolc::blocking_install(&default_version)?;
            zksolc = ZkSolc::find_installed_version(&default_version)?;
        }
        zksolc
            .map(|c| c.zksolc)
            .unwrap_or_else(|| panic!("Could not install zksolc v{}", default_version))
    } else {
        "zksolc".into()
    };

    let zksolc_compiler = ZkSolcCompiler { zksolc, solc: config.solc_compiler()? };

    let project = builder.build(zksolc_compiler)?;

    if config.force {
        config.cleanup(&project)?;
    }

    Ok(project)
}
/// Returns the `ProjectPathsConfig` sub set of the config.
pub fn config_project_paths(config: &Config) -> ProjectPathsConfig<SolcLanguage> {
    let builder = ProjectPathsConfig::builder()
        .cache(config.cache_path.join(ZKSYNC_SOLIDITY_FILES_CACHE_FILENAME))
        .sources(&config.src)
        .tests(&config.test)
        .scripts(&config.script)
        .artifacts(config.root.0.join("zkout"))
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
                zksolc.map(|commmand| commmand.zksolc)
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
