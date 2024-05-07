use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use tracing::{debug, trace, warn};

use foundry_compilers::info::ContractInfo;

use crate::ZkMissingLibrary;

/// Manages non-inlineable libraries for zkSolc
pub struct ZkLibrariesManager;

impl ZkLibrariesManager {
    /// Return the missing libraries cache path
    pub(crate) fn get_missing_libraries_cache_path(project_root: &Path) -> PathBuf {
        project_root.join(".zksolc-libraries-cache/missing_library_dependencies.json")
    }

    /// Add libraries to missing libraries cache
    pub(crate) fn add_dependencies_to_missing_libraries_cache(
        project_root: &Path,
        libraries: &[ZkMissingLibrary],
    ) -> eyre::Result<()> {
        let file_path = Self::get_missing_libraries_cache_path(project_root);
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::File::create(file_path)?
            .write_all(serde_json::to_string_pretty(libraries).unwrap().as_bytes())?;
        Ok(())
    }

    /// Returns the detected missing libraries from previous compilation
    pub fn get_detected_missing_libraries(
        project_root: &Path,
    ) -> eyre::Result<Vec<ZkMissingLibrary>> {
        let library_paths = Self::get_missing_libraries_cache_path(project_root);
        if !library_paths.exists() {
            eyre::bail!("No missing libraries found");
        }

        Ok(serde_json::from_reader(fs::File::open(&library_paths)?)?)
    }

    /// Performs cleanup of cached missing libraries
    pub fn cleanup_detected_missing_libraries(project_root: &Path) -> eyre::Result<()> {
        fs::remove_file(Self::get_missing_libraries_cache_path(project_root))?;
        Ok(())
    }

    /// Retrieve ordered list of libraries to deploy
    pub fn resolve_libraries(
        mut missing_libraries: Vec<ZkMissingLibrary>,
        already_deployed_libraries: &[ContractInfo],
    ) -> eyre::Result<Vec<ContractInfo>> {
        trace!(?missing_libraries, ?already_deployed_libraries, "filtering out missing libraries");
        missing_libraries.retain(|lib| {
            !already_deployed_libraries.iter().any(|dep| {
                dep.name == lib.contract_name &&
                    dep.path.as_ref().map(|path| path == &lib.contract_path).unwrap_or(true)
            })
        });

        let mut output_contracts = Vec::with_capacity(missing_libraries.len());
        loop {
            if missing_libraries.is_empty() {
                break Ok(output_contracts);
            }

            let Some(next_lib) = missing_libraries
                .iter()
                .enumerate()
                .find(|(_, lib)| lib.missing_libraries.is_empty())
                .map(|(i, _)| i)
                .map(|i| missing_libraries.remove(i))
            else {
                warn!(?missing_libraries, "unable to find library ready to be deployed");
                eyre::bail!("Library dependency cycle detected");
            };

            //remove this lib from each missing_library if listed as dependency
            for lib in &mut missing_libraries {
                lib.missing_libraries.retain(|maybe_missing_lib| {
                    let mut split = maybe_missing_lib.split(':');
                    let lib_path = split.next().unwrap();
                    let lib_name = split.next().unwrap();

                    let r =
                        !(next_lib.contract_path == lib_path && next_lib.contract_name == lib_name);
                    if !r {
                        debug!(
                            name = lib.contract_name,
                            dependency = lib_name,
                            "deployed library dependency"
                        )
                    }
                    r
                })
            }

            let info =
                ContractInfo { path: Some(next_lib.contract_path), name: next_lib.contract_name };
            output_contracts.push(info);
        }
    }
}
