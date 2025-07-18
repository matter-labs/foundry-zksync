use super::{EtherscanSourceProvider, EtherscanZksyncSourceProvider, VerifyArgs};
use crate::{
    provider::VerificationContext, verify::ContractLanguage, zk_provider::ZkVerificationContext,
};
use eyre::{Context, Result};
use foundry_block_explorers::verify::CodeFormat;
use foundry_compilers::{
    artifacts::{Source, StandardJsonCompilerInput, vyper::VyperInput},
    solc::SolcLanguage,
};
use std::path::Path;

#[derive(Debug)]
pub struct EtherscanStandardJsonSource;
impl EtherscanSourceProvider for EtherscanStandardJsonSource {
    fn source(
        &self,
        args: &VerifyArgs,
        context: &VerificationContext,
    ) -> Result<(String, String, CodeFormat)> {
        let mut input: StandardJsonCompilerInput = context
            .project
            .standard_json_input(&context.target_path)
            .wrap_err("Failed to get standard json input")?
            .normalize_evm_version(&context.compiler_version);

        let lang = args.detect_language(context);

        let code_format = match lang {
            ContractLanguage::Solidity => CodeFormat::StandardJsonInput,
            ContractLanguage::Vyper => CodeFormat::VyperJson,
        };

        let mut settings = context.compiler_settings.solc.settings.clone();
        settings.libraries.libs = input
            .settings
            .libraries
            .libs
            .into_iter()
            .map(|(f, libs)| {
                (f.strip_prefix(context.project.root()).unwrap_or(&f).to_path_buf(), libs)
            })
            .collect();

        settings.remappings = input.settings.remappings;

        // remove all incompatible settings
        settings.sanitize(&context.compiler_version, SolcLanguage::Solidity);

        input.settings = settings;

        let source = match lang {
            ContractLanguage::Solidity => {
                serde_json::to_string(&input).wrap_err("Failed to parse standard json input")?
            }
            ContractLanguage::Vyper => {
                let path = Path::new(&context.target_path);
                let sources = Source::read_all_from(path, &["vy", "vyi"])?;
                let input = VyperInput::new(
                    sources,
                    context.clone().compiler_settings.vyper,
                    &context.compiler_version,
                );

                serde_json::to_string(&input).wrap_err("Failed to parse vyper json input")?
            }
        };

        trace!(target: "forge::verify", standard_json=source, "determined standard json input");

        let name = format!(
            "{}:{}",
            context
                .target_path
                .strip_prefix(context.project.root())
                .unwrap_or(context.target_path.as_path())
                .display(),
            context.target_name.clone()
        );
        Ok((source, name, code_format))
    }
}

impl EtherscanZksyncSourceProvider for EtherscanStandardJsonSource {
    fn zksync_source(
        &self,
        _args: &VerifyArgs,
        context: &ZkVerificationContext,
    ) -> Result<(String, String, CodeFormat)> {
        let input = foundry_zksync_compilers::compilers::project_standard_json_input(
            &context.project,
            &context.target_path,
        )
        .wrap_err("failed to get zksolc standard json")?;

        let source =
            serde_json::to_string(&input).wrap_err("Failed to parse zksync standard json input")?;

        trace!(target: "forge::verify", standard_json=source, "determined zksync standard json input");

        let name = format!(
            "{}:{}",
            context
                .target_path
                .strip_prefix(context.project.root())
                .unwrap_or(context.target_path.as_path())
                .display(),
            context.target_name.clone()
        );
        Ok((source, name, CodeFormat::StandardJsonInput))
    }
}
