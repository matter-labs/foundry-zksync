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
    contract_name: &str,
) -> Result<()> {
    let project = foundry_config::zksync::config_create_project(&config, false, true)?;
    let compiler = ProjectCompiler::new().quiet(true);
    let artifact = compiler
        .files([target_path.clone()])
        .zksync_compile(&project)?
        .remove(&target_path, contract_name)
        .ok_or_else(|| {
            eyre::eyre!("Could not find artifact `{}` in the compiled artifacts", contract_name)
        })?;

    if matches!(field, ContractArtifactField::Bytecode | ContractArtifactField::DeployedBytecode) {
        print_json_str(&artifact.bytecode, Some("object"))?;
    }

    Ok(())
}
