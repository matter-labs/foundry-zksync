//! # foundry-zksync
//!
//! Main Foundry ZKSync implementation.
#![warn(missing_docs, unused_crate_dependencies)]

/// ZKSolc specific logic.
mod zksolc;

use std::path::Path;

use foundry_config::{Config, SkipBuildFilters, SolcReq};
pub use zksolc::*;

pub mod libraries;

use foundry_compilers::{
    artifacts::Severity, error::SolcError, solc::SolcCompiler, zksolc::ZkSolc,
    zksync::config::ZkSolcConfig, Compiler, Project, ProjectBuilder,
};

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
            let default_version = semver::Version::new(1, 5, 1);
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

/// Obtain a standard json input for zksolc
pub fn standard_json_input<C: Compiler>(
    project: &Project<C>,
    target_path: impl AsRef<Path>,
) -> Result<serde_json::Value, SolcError>
where
    C::Settings: Into<foundry_compilers::artifacts::Settings>,
{
    let mut input = project.standard_json_input(target_path)?;
    tracing::debug!(?input.settings.remappings, "standard_json_input for zksync");

    let mut settings = project.zksync_zksolc_config.settings.clone();
    settings.remappings = std::mem::take(&mut input.settings.remappings);
    settings.libraries.libs = settings
        .libraries
        .libs
        .into_iter()
        .map(|(f, libs)| (f.strip_prefix(project.root()).unwrap_or(&f).to_path_buf(), libs))
        .collect();
    let settings = serde_json::to_value(settings).expect("able to serialize settings as json");

    let mut serialized = serde_json::to_value(input).expect("able to serialize input as json");
    serialized["settings"] = settings;

    Ok(serialized)
}
