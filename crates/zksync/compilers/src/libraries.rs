//! Handles resolution and storage of missing libraries emitted by zksolc

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use tracing::{trace, warn};

use foundry_compilers::info::ContractInfo;

/// Missing Library entry
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ZkMissingLibrary {
    /// Contract name
    pub contract_name: String,
    /// Contract path
    pub contract_path: String,
    /// Missing Libraries
    pub missing_libraries: Vec<String>,
}

/// Return the missing libraries cache path
pub(crate) fn get_missing_libraries_cache_path(project_root: impl AsRef<Path>) -> PathBuf {
    project_root.as_ref().join(".zksolc-libraries-cache/missing_library_dependencies.json")
}

/// Add libraries to missing libraries cache
pub fn add_dependencies_to_missing_libraries_cache(
    project_root: impl AsRef<Path>,
    libraries: &[ZkMissingLibrary],
) -> eyre::Result<()> {
    let file_path = get_missing_libraries_cache_path(project_root);
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    fs::File::create(file_path)?
        .write_all(serde_json::to_string_pretty(libraries).unwrap().as_bytes())?;
    Ok(())
}

/// Returns the detected missing libraries from previous compilation
pub fn get_detected_missing_libraries(
    project_root: impl AsRef<Path>,
) -> eyre::Result<Vec<ZkMissingLibrary>> {
    let library_paths = get_missing_libraries_cache_path(project_root);
    if !library_paths.exists() {
        eyre::bail!("No missing libraries found");
    }

    Ok(serde_json::from_reader(fs::File::open(&library_paths)?)?)
}

/// Performs cleanup of cached missing libraries
pub fn cleanup_detected_missing_libraries(project_root: impl AsRef<Path>) -> eyre::Result<()> {
    fs::remove_file(get_missing_libraries_cache_path(project_root))?;
    Ok(())
}

/// Retrieve ordered list of libraries to deploy
///
/// Libraries are grouped in batches, where the next batch
/// may have dependencies on the previous one, thus
/// it's recommended to build & deploy one batch before moving onto the next
pub fn resolve_libraries(
    mut missing_libraries: Vec<ZkMissingLibrary>,
    already_deployed_libraries: &[ContractInfo],
) -> eyre::Result<Vec<Vec<ContractInfo>>> {
    trace!(?missing_libraries, ?already_deployed_libraries, "filtering out missing libraries");
    missing_libraries.retain(|lib| {
        !already_deployed_libraries.contains(&ContractInfo {
            path: Some(lib.contract_path.to_string()),
            name: lib.contract_name.to_string(),
        })
    });

    let mut batches = Vec::new();
    loop {
        if missing_libraries.is_empty() {
            break Ok(batches);
        }

        let mut batch = Vec::new();
        loop {
            // find library with no further dependencies
            let Some(next_lib) = missing_libraries
                .iter()
                .enumerate()
                .find(|(_, lib)| lib.missing_libraries.is_empty())
                .map(|(i, _)| i)
                .map(|i| missing_libraries.remove(i))
            else {
                // no such library, and we didn't collect any library already
                if batch.is_empty() {
                    warn!(
                        ?missing_libraries,
                        ?batches,
                        "unable to find library ready to be deployed"
                    );
                    //TODO: determine if this error message is accurate
                    eyre::bail!("Library dependency cycle detected");
                }

                break;
            };

            let info =
                ContractInfo { path: Some(next_lib.contract_path), name: next_lib.contract_name };
            batch.push(info);
        }

        // remove this batch from each library's missing_library if listed as dependency
        // this potentially allows more libraries to be included in the next batch
        for lib in &mut missing_libraries {
            lib.missing_libraries.retain(|maybe_missing_lib| {
                let mut split = maybe_missing_lib.split(':');
                let lib_path = split.next().unwrap();
                let lib_name = split.next().unwrap();

                !batch.contains(&ContractInfo {
                    path: Some(lib_path.to_string()),
                    name: lib_name.to_string(),
                })
            })
        }

        batches.push(batch);
    }
}
