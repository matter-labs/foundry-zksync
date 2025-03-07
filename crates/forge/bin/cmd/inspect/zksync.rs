use eyre::Result;
use foundry_common::compile::ProjectCompiler;
use foundry_config::Config;
use std::path::PathBuf;

use super::{print_json_str, ContractArtifactField};

pub fn check_command_for_field(field: &ContractArtifactField) -> Result<bool> {
    // NOTE(zk): Fields that should have specific behavior for zksolc
    // TODO(zk): we should eventually migrate all fields from fields_zksolc_unimplemented_warn
    // to this array
    let fields_zksolc_specific_behavior =
        [ContractArtifactField::Bytecode, ContractArtifactField::DeployedBytecode];

    let fields_zksolc_unimplemented_warn = [
        ContractArtifactField::GasEstimates,
        ContractArtifactField::StorageLayout,
        ContractArtifactField::Metadata,
        ContractArtifactField::Eof,
        ContractArtifactField::EofInit,
    ];

    let fields_zksolc_should_error = [
        ContractArtifactField::Assembly,
        ContractArtifactField::AssemblyOptimized,
        ContractArtifactField::LegacyAssembly,
        ContractArtifactField::Ir,
        ContractArtifactField::IrOptimized,
        ContractArtifactField::Ewasm,
    ];

    if fields_zksolc_should_error.contains(field) {
        return Err(eyre::eyre!("ZKsync version of inspect does not support this field"));
    }

    if fields_zksolc_unimplemented_warn.contains(field) {
        return Err(eyre::eyre!(
            "This field has not been implemented for zksolc yet, so defaulting to solc implementation"
        ));
    }

    Ok(fields_zksolc_specific_behavior.contains(field))
}

pub fn inspect(
    field: &ContractArtifactField,
    config: Config,
    target_path: PathBuf,
    contract_name: Option<&str>,
) -> Result<()> {
    let project = foundry_config::zksync::config_create_project(&config, false, true)?;
    let compiler = ProjectCompiler::new().quiet(true);
    let output = compiler.files([target_path.clone()]).zksync_compile(&project)?;

    let artifact = match contract_name {
        Some(name) => output.find(&target_path, name).ok_or_else(|| {
            eyre::eyre!("Could not find artifact `{name}` in the compiled artifacts",)
        }),
        None => {
            let possible_targets = output
                .artifact_ids()
                .filter(|(id, _artifact)| id.source == target_path)
                .collect::<Vec<_>>();

            if possible_targets.is_empty() {
                eyre::bail!("Could not find artifact linked to source `{target_path:?}` in the compiled artifacts");
            }

            let (target_id, target_artifact) = possible_targets[0].clone();
            if possible_targets.len() == 1 {
                Ok(target_artifact)
            } else {
                // If all artifact_ids in `possible_targets` have the same name (without ".",
                // indicates additional compiler profiles), it means that there are
                // multiple contracts in the same file.
                if !target_id.name.contains(".") &&
                    possible_targets.iter().any(|(id, _)| id.name != target_id.name)
                {
                    eyre::bail!("Multiple contracts found in the same file, please specify the target <path>:<contract> or <contract>");
                }

                // Otherwise, we're dealing with additional compiler profiles wherein `id.source` is
                // the same but `id.path` is different.
                let artifact = possible_targets
                    .iter()
                    .find_map(
                        |(id, artifact)| {
                            if id.profile == "default" {
                                Some(*artifact)
                            } else {
                                None
                            }
                        },
                    )
                    .unwrap_or(target_artifact);
                Ok(artifact)
            }
        }
    }?;

    if matches!(field, ContractArtifactField::Bytecode | ContractArtifactField::DeployedBytecode) {
        print_json_str(&artifact.bytecode, Some("object"))?;
    }

    Ok(())
}
