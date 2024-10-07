use super::{ZksyncSourceProvider, VerifyArgs, ZkSourceOutput};
use crate::{
    provider::VerificationContext,
    zk_provider::{ZkVerificationContext, ZkVersion},
};
use eyre::{Context, Result};
use foundry_block_explorers::verify::CodeFormat;
use foundry_compilers::{
    artifacts::{BytecodeHash, Source, Sources},
    buildinfo::RawBuildInfo,
    compilers::{
        solc::{SolcCompiler, SolcLanguage, SolcVersionedInput},
        Compiler, CompilerInput,
    },
    solc::{CliSettings, Solc},
    zksolc::{
        input::{ZkSolcInput, ZkSolcVersionedInput},
        ZkSolc, ZkSolcCompiler,
    },
    zksync::{
        compile::output::AggregatedCompilerOutput as ZkAggregatedCompilerOutput, raw_build_info_new,
    },
    AggregatedCompilerOutput,
};
use semver::{BuildMetadata, Version};
use std::path::Path;

#[derive(Debug)]
pub struct ZksyncFlattenedSource;
impl ZksyncSourceProvider for ZksyncFlattenedSource {
    fn zk_source(
        &self,
        args: &VerifyArgs,
        context: &ZkVerificationContext,
    ) -> Result<(ZkSourceOutput, String, CodeFormat)> {
        let metadata = context.project.settings.settings.metadata.as_ref();
        //let bch = metadata.and_then(|m| m.bytecode_hash).unwrap_or_default();

        // eyre::ensure!(
        //     bch == foundry_compilers::zksolc::settings::BytecodeHash::Keccak256,
        //     "When using flattened source with zksync, bytecodeHash must be set to keccak256 because Etherscan uses Keccak256 in its Compiler Settings when re-compiling your code. BytecodeHash is currently: {}. Hint: Set the bytecodeHash key in your foundry.toml :)",
        //     bch,
        // );
        println!("before source");
        let source = context
            .project
            .paths
            .clone()
            .with_language::<SolcLanguage>()
            .flatten(&context.target_path)
            .wrap_err("Failed to flatten contract")?;
        
        
        println!("fails gere?:");


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

        let relative_path = context
            .target_path
            .strip_prefix(context.project.root())
            .unwrap_or(context.target_path.as_path())
            .display()
            .to_string();
        let normalized_path = relative_path.replace('\\', "/");

        // Format the name as <path>/<file>:<contract_name>
        let name = format!("{}:{}", normalized_path, context.target_name);

        Ok((ZkSourceOutput::FlattenedSource(source), name, CodeFormat::SingleFile))
    }
}

impl ZksyncFlattenedSource {
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
        println!("zksolc version??? {}", zksolc_version);
        println!("solc version??? {}", solc_version);
        
        let zksolc = ZkSolc::find_installed_version(&zksolc_version)?
            .unwrap_or(ZkSolc::blocking_install(&zksolc_version)?);
        println!("do we get here?");
        let input = ZkSolcVersionedInput {
            input: ZkSolcInput {
                language: SolcLanguage::Solidity,
                sources: Sources::from([("contract.sol".into(), Source::new(content))]),
                ..Default::default()
            },
            solc_version: solc_version.clone(),
            cli_settings: CliSettings::default(),
        };
        //let solc_version2 = format!("", &solc_version);
        let zksync_solc = ZkSolc::solc_blocking_install("0.8.17".into())?;
        let ss = Solc::new(zksync_solc)?;
        let solc_compiler = SolcCompiler::Specific(ss);
        println!("how about here?");
        // let mut solc_compiler = if compiler_version.is_zksync_solc {
        //     // AutoDetect given a specific solc version on the input, will
        //     // find or install the solc version
        //     println!("auto detecht");
        //     let solc = Solc::find_or_install(&solc_version)?;
        //     SolcCompiler::Specific(solc)
        // } else {
        //     println!("specific");
        //     println!("solc version??? {}", solc_version);
        //     let solc = Solc::find_or_install(&solc_version)?;
        //     SolcCompiler::Specific(solc)
        // };

        let zksolc_compiler = ZkSolcCompiler { zksolc: zksolc.zksolc, solc: solc_compiler };
        println!("zksolc compiler??? {:?}", zksolc_compiler);
        let out = zksolc_compiler.zksync_compile(&input)?;
        if out.has_error() {
            if !out.errors.is_empty() {
                for error in &out.errors {
                    // Assuming `error` has a `message` or similar field for error details
                    error!("Compilation error: {:?}", error);
                }
            }
            let mut o = ZkAggregatedCompilerOutput::default();
            o.extend(solc_version, raw_build_info_new(&input, &out, false)?, out);
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
