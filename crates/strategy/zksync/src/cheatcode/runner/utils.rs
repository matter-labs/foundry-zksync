use std::{fs, path::PathBuf, sync::Arc};

use alloy_json_abi::ContractObject;
use alloy_primitives::Bytes;
use foundry_cheatcodes::{CheatsConfig, Error, Result};
use foundry_config::fs_permissions::FsAccessKind;
use foundry_zksync_compilers::dual_compiled_contracts::{ContractType, DualCompiledContracts};
use semver::Version;

pub(super) fn get_artifact_code(
    dual_compiled_contracts: &DualCompiledContracts,
    using_zk_vm: bool,
    config: &Arc<CheatsConfig>,
    path: &str,
    deployed: bool,
) -> Result<Bytes> {
    let path = if path.ends_with(".json") {
        PathBuf::from(path)
    } else {
        let mut parts = path.split(':');

        let mut file = None;
        let mut contract_name = None;
        let mut version = None;

        let path_or_name = parts.next().unwrap();
        if path_or_name.contains('.') {
            file = Some(PathBuf::from(path_or_name));
            if let Some(name_or_version) = parts.next() {
                if name_or_version.contains('.') {
                    version = Some(name_or_version);
                } else {
                    contract_name = Some(name_or_version);
                    version = parts.next();
                }
            }
        } else {
            contract_name = Some(path_or_name);
            version = parts.next();
        }

        let version = if let Some(version) = version {
            Some(
                Version::parse(version)
                    .map_err(|e| Error::display(format!("failed parsing version: {e}")))?,
            )
        } else {
            None
        };

        // Use available artifacts list if present
        if let Some(artifacts) = &config.available_artifacts {
            let filtered = artifacts
                .iter()
                .filter(|(id, _)| {
                    // name might be in the form of "Counter.0.8.23"
                    let id_name = id.name.split('.').next().unwrap();

                    if let Some(path) = &file {
                        if !id.source.ends_with(path) {
                            return false;
                        }
                    }
                    if let Some(name) = contract_name {
                        if id_name != name {
                            return false;
                        }
                    }
                    if let Some(ref version) = version {
                        if id.version.minor != version.minor ||
                            id.version.major != version.major ||
                            id.version.patch != version.patch
                        {
                            return false;
                        }
                    }
                    true
                })
                .collect::<Vec<_>>();

            let artifact = match &filtered[..] {
                [] => Err(Error::display("no matching artifact found")),
                [artifact] => Ok(artifact),
                filtered => {
                    // If we find more than one artifact, we need to filter by contract type
                    // depending on whether we are using the zkvm or evm
                    filtered
                        .iter()
                        .find(|(id, _)| {
                            let contract_type =
                                dual_compiled_contracts.get_contract_type_by_artifact(id);
                            match contract_type {
                                Some(ContractType::ZK) => using_zk_vm,
                                Some(ContractType::EVM) => !using_zk_vm,
                                None => false,
                            }
                        })
                        .or_else(|| {
                            // If we know the current script/test contract solc version, try to
                            // filter by it
                            config.running_artifact.as_ref().and_then(|artifact| {
                                filtered.iter().find(|(id, _)| id.version == artifact.version)
                            })
                        })
                        .ok_or_else(|| Error::display("multiple matching artifacts found"))
                }
            }?;

            let maybe_bytecode = if deployed {
                artifact.1.deployed_bytecode().cloned()
            } else {
                artifact.1.bytecode().cloned()
            };

            return maybe_bytecode.ok_or_else(|| {
                Error::display("no bytecode for contract; is it abstract or unlinked?")
            });
        } else {
            let path_in_artifacts =
                match (file.map(|f| f.to_string_lossy().to_string()), contract_name) {
                    (Some(file), Some(contract_name)) => {
                        PathBuf::from(format!("{file}/{contract_name}.json"))
                    }
                    (None, Some(contract_name)) => {
                        PathBuf::from(format!("{contract_name}.sol/{contract_name}.json"))
                    }
                    (Some(file), None) => {
                        let name = file.replace(".sol", "");
                        PathBuf::from(format!("{file}/{name}.json"))
                    }
                    _ => return Err(Error::display("invalid artifact path")),
                };

            config.paths.artifacts.join(path_in_artifacts)
        }
    };

    let path = config.ensure_path_allowed(path, FsAccessKind::Read)?;
    let data = fs::read_to_string(path)?;
    let artifact = serde_json::from_str::<ContractObject>(&data)?;
    let maybe_bytecode = if deployed { artifact.deployed_bytecode } else { artifact.bytecode };
    maybe_bytecode
        .ok_or_else(|| Error::display("no bytecode for contract; is it abstract or unlinked?"))
}
