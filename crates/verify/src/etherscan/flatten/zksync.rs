use std::path::Path;

use eyre::{Context, Result};
use foundry_block_explorers::verify::CodeFormat;
use foundry_compilers::{
    artifacts::{Source, Sources},
    buildinfo::RawBuildInfo,
    solc::{CliSettings, Solc, SolcCompiler, SolcLanguage},
    AggregatedCompilerOutput, Compiler,
};
use foundry_zksync_compilers::compilers::zksolc::{
    input::{ZkSolcInput, ZkSolcVersionedInput},
    ZkSolc, ZkSolcCompiler,
};

use crate::{
    etherscan::zksync::EtherscanZksyncSourceProvider,
    zk_provider::{ZkVerificationContext, ZkVersion},
    VerifyArgs,
};

use super::{strip_build_meta, EtherscanFlattenedSource};

impl EtherscanZksyncSourceProvider for EtherscanFlattenedSource {
    fn zksync_source(
        &self,
        args: &VerifyArgs,
        context: &ZkVerificationContext,
    ) -> Result<(String, String, CodeFormat)> {
        let metadata = context.project.settings.settings.metadata.as_ref();
        let bch = metadata.and_then(|m| m.hash_type).unwrap_or_default();

        eyre::ensure!(
            bch == foundry_zksync_compilers::compilers::zksolc::settings::BytecodeHash::Keccak256,
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
    /// Attempts to compile the flattened content locally with the zksolc and solc compiler
    /// versions.
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
    /// If the zksolc compiler output contains errors, this could either be due to a bug in the
    /// flattening code or could to conflict in the flattened code, for example if there are
    /// multiple interfaces with the same name.
    fn zk_check_flattened(
        &self,
        content: impl Into<String>,
        compiler_version: &ZkVersion,
        contract_path: &Path,
    ) -> Result<()> {
        let solc_version = strip_build_meta(compiler_version.solc.clone());
        let zksolc_version = strip_build_meta(compiler_version.zksolc.clone());
        let zksolc_path = ZkSolc::find_installed_version(&zksolc_version)?
            .unwrap_or(ZkSolc::blocking_install(&solc_version)?);

        let input = ZkSolcVersionedInput {
            input: ZkSolcInput {
                language: SolcLanguage::Solidity,
                sources: Sources::from([("contract.sol".into(), Source::new(content))]),
                ..Default::default()
            },
            solc_version: solc_version.clone(),
            cli_settings: CliSettings::default(),
            zksolc_path,
        };

        let solc_compiler = if compiler_version.is_zksync_solc {
            // AutoDetect given a specific solc version on the input, will
            // find or install the solc version
            SolcCompiler::AutoDetect
        } else {
            let solc = Solc::find_or_install(&solc_version)?;
            SolcCompiler::Specific(solc)
        };

        let zksolc_compiler = ZkSolcCompiler { solc: solc_compiler };

        let out = zksolc_compiler.compile(&input)?;
        if out.errors.iter().any(|e| e.is_error()) {
            let mut o: AggregatedCompilerOutput<ZkSolcCompiler> =
                AggregatedCompilerOutput::default();
            o.extend(solc_version, RawBuildInfo::new(&input, &out, false)?, "default", out);
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
