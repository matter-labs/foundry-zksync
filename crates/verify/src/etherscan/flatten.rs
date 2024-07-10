use super::{EtherscanSourceProvider, VerifyArgs};
use crate::provider::VerificationContext;
use eyre::{Context, Result};
use foundry_block_explorers::verify::CodeFormat;
use foundry_compilers::{
    artifacts::{BytecodeHash, Source},
    buildinfo::RawBuildInfo,
    compilers::{
        solc::{SolcCompiler, SolcLanguage, SolcVersionedInput},
        Compiler, CompilerInput,
    },
    solc::Solc,
    zksolc::{
        input::{ZkSolcInput, ZkSolcVersionedInput},
        ZkSolc,
    },
    zksync::{
        compile::output::AggregatedCompilerOutput as ZkAggregatedCompilerOutput, raw_build_info_new,
    },
    AggregatedCompilerOutput,
};
use semver::{BuildMetadata, Version};

use std::{collections::BTreeMap, path::Path};

#[derive(Debug)]
pub struct EtherscanFlattenedSource;
impl EtherscanSourceProvider for EtherscanFlattenedSource {
    fn source(
        &self,
        args: &VerifyArgs,
        context: &VerificationContext,
    ) -> Result<(String, String, CodeFormat)> {
        let metadata = context.project.settings.solc.metadata.as_ref();
        let bch = metadata.and_then(|m| m.bytecode_hash).unwrap_or_default();

        eyre::ensure!(
            bch == BytecodeHash::Ipfs,
            "When using flattened source, bytecodeHash must be set to ipfs because Etherscan uses IPFS in its Compiler Settings when re-compiling your code. BytecodeHash is currently: {}. Hint: Set the bytecodeHash key in your foundry.toml :)",
            bch,
        );

        let source = context
            .project
            .paths
            .clone()
            .with_language::<SolcLanguage>()
            .flatten(&context.target_path)
            .wrap_err("Failed to flatten contract")?;

        if !args.force {
            // solc dry run of flattened code
            self.check_flattened(source.clone(), &context.compiler_version, &context.target_path)
                .map_err(|err| {
                eyre::eyre!(
                    "Failed to compile the flattened code locally: `{}`\
            To skip this solc dry, have a look at the `--force` flag of this command.",
                    err
                )
            })?;
        }

        Ok((source, context.target_name.clone(), CodeFormat::SingleFile))
    }

    fn zk_source(
        &self,
        args: &VerifyArgs,
        context: &VerificationContext,
    ) -> Result<(String, String, CodeFormat)> {
        let metadata = context.project.zksync_zksolc_config.settings.metadata.as_ref();
        let bch = metadata.and_then(|m| m.bytecode_hash).unwrap_or_default();

        eyre::ensure!(
            bch == foundry_compilers::zksolc::settings::BytecodeHash::Keccak256,
            "When using flattened source with zksync, bytecodeHash must be set to keccak256 because Etherscan uses Keccak256 in its Compiler Settings when re-compiling your code. BytecodeHash is currently: {}. Hint: Set the bytecodeHash key in your foundry.toml :)",
            bch,
        );

        let source = context
            .project
            .paths
            .clone()
            .with_language::<SolcLanguage>()
            .flatten(&context.target_path)
            .wrap_err("Failed to flatten contract")?;

        if !args.force {
            // solc dry run of flattened code
            self.zk_check_flattened(
                source.clone(),
                &context.compiler_version,
                &context.target_path,
            )
            .map_err(|err| {
                eyre::eyre!(
                    "Failed to compile the flattened code locally: `{}`\
            To skip this solc dry, have a look at the `--force` flag of this command.",
                    err
                )
            })?;
        }

        Ok((source, context.target_name.clone(), CodeFormat::SingleFile))
    }
}

impl EtherscanFlattenedSource {
    /// Attempts to compile the flattened content locally with the compiler version.
    ///
    /// This expects the completely flattened content and will try to compile it using the
    /// provided compiler. If the compiler is missing it will be installed.
    ///
    /// # Errors
    ///
    /// If it failed to install a missing solc compiler
    ///
    /// # Exits
    ///
    /// If the solc compiler output contains errors, this could either be due to a bug in the
    /// flattening code or could to conflict in the flattened code, for example if there are
    /// multiple interfaces with the same name.
    fn check_flattened(
        &self,
        content: impl Into<String>,
        version: &Version,
        contract_path: &Path,
    ) -> Result<()> {
        let version = strip_build_meta(version.clone());
        let solc = Solc::find_or_install(&version)?;

        let input = SolcVersionedInput::build(
            BTreeMap::from([("contract.sol".into(), Source::new(content))]),
            Default::default(),
            SolcLanguage::Solidity,
            version.clone(),
        );

        let out = SolcCompiler::Specific(solc).compile(&input)?;
        if out.errors.iter().any(|e| e.is_error()) {
            let mut o = AggregatedCompilerOutput::<SolcCompiler>::default();
            o.extend(version.clone(), RawBuildInfo::new(&input, &out, false)?, out);
            let diags = o.diagnostics(&[], &[], Default::default());

            eyre::bail!(
                "\
Failed to compile the flattened code locally.
This could be a bug, please inspect the output of `forge flatten {}` and report an issue.
To skip this solc dry, pass `--force`.
Diagnostics: {diags}",
                contract_path.display()
            );
        }

        Ok(())
    }

    /// Attempts to compile the flattened content locally with the zksolc compiler version.
    ///
    /// This expects the completely flattened `content´ and will try to compile it using the
    /// provided compiler. If the compiler is missing it will be installed.
    ///
    /// # Errors
    ///
    /// If it failed to install a missing solc compiler
    ///
    /// # Exits
    ///
    /// If the solc compiler output contains errors, this could either be due to a bug in the
    /// flattening code or could to conflict in the flattened code, for example if there are
    /// multiple interfaces with the same name.
    fn zk_check_flattened(
        &self,
        content: impl Into<String>,
        version: &Version,
        contract_path: &Path,
    ) -> Result<()> {
        let version = strip_build_meta(version.clone());
        let zksolc = ZkSolc::find_installed_version(&version)?
            .unwrap_or(ZkSolc::blocking_install(&version)?);

        let mut input = ZkSolcVersionedInput {
            input: ZkSolcInput {
                language: SolcLanguage::Solidity,
                sources: BTreeMap::from([("contract.sol".into(), Source::new(content))]),
                ..Default::default()
            },
            solc_version: version.clone(),
            allow_paths: Default::default(),
            base_path: Default::default(),
            include_paths: Default::default(),
        };

        let out = zksolc.compile(&mut input)?;
        if out.has_error() {
            let mut o = ZkAggregatedCompilerOutput::default();
            o.extend(version.clone(), raw_build_info_new(&input, &out, false)?, out);
            let diags = o.diagnostics(&[], &[], Default::default());

            eyre::bail!(
                "\
Failed to compile the flattened code locally.
This could be a bug, please inspect the output of `forge flatten {}` and report an issue.
To skip this zksolc dry, pass `--force`.
Diagnostics: {diags}",
                contract_path.display()
            );
        }

        Ok(())
    }
}

/// Strips [BuildMetadata] from the [Version]
///
/// **Note:** this is only for local compilation as a dry run, therefore this will return a
/// sanitized variant of the specific version so that it can be installed. This is merely
/// intended to ensure the flattened code can be compiled without errors.
fn strip_build_meta(version: Version) -> Version {
    if version.build != BuildMetadata::EMPTY {
        Version::new(version.major, version.minor, version.patch)
    } else {
        version
    }
}
