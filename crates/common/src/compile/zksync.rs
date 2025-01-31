use std::{collections::BTreeMap, io::IsTerminal};

use comfy_table::Color;
use eyre::Result;
use foundry_compilers::{
    artifacts::Source,
    report::{BasicStdoutReporter, NoReporter, Report},
    Project, ProjectCompileOutput,
};
use foundry_zksync_compilers::compilers::{
    artifact_output::zk::ZkArtifactOutput,
    zksolc::{ZkSolc, ZkSolcCompiler, ZKSOLC_UNSUPPORTED_VERSIONS},
};

use crate::{reports::report_kind, shell, term::SpinnerReporter, TestFunctionExt};

use super::{contract_size, ContractInfo, ProjectCompiler, SizeReport};

// https://docs.zksync.io/build/developer-reference/ethereum-differences/contract-deployment#contract-size-limit-and-format-of-bytecode-hash
pub(super) const ZKSYNC_CONTRACT_SIZE_LIMIT: usize = 450999;

impl ProjectCompiler {
    /// Enables zksync contract sizes.
    #[inline]
    pub fn zksync_sizes(mut self) -> Self {
        self.zksync = true;
        self
    }

    /// Compiles the project.
    pub fn zksync_compile(
        self,
        project: &Project<ZkSolcCompiler, ZkArtifactOutput>,
    ) -> Result<ProjectCompileOutput<ZkSolcCompiler, ZkArtifactOutput>> {
        // TODO: Avoid process::exit
        if !project.paths.has_input_files() && self.files.is_empty() {
            sh_println!("Nothing to compile")?;
            // nothing to do here
            std::process::exit(0);
        }

        // Taking is fine since we don't need these in `compile_with`.
        //let filter = std::mem::take(&mut self.filter);

        // We need to clone files since we use them in `compile_with`
        // for filtering artifacts in missing libraries detection
        let files = self.files.clone();

        {
            let zksolc_current_version = project.settings.zksolc_version_ref();
            let zksolc_min_supported_version = ZkSolc::zksolc_minimum_supported_version();
            let zksolc_latest_supported_version = ZkSolc::zksolc_latest_supported_version();
            if ZKSOLC_UNSUPPORTED_VERSIONS.contains(zksolc_current_version) {
                sh_warn!("Compiling with zksolc v{zksolc_current_version} which is not supported and may lead to unexpected errors. Specifying an unsupported version is deprecated and will return an error in future versions of foundry-zksync.")?;
            }
            if zksolc_current_version < &zksolc_min_supported_version {
                sh_warn!("Compiling with zksolc v{zksolc_current_version} which is not supported and may lead to unexpected errors. Specifying an unsupported version is deprecated and will return an error in future versions of foundry-zksync. Minimum version supported is v{zksolc_min_supported_version}")?;
            }
            if zksolc_current_version > &zksolc_latest_supported_version {
                sh_warn!("Compiling with zksolc v{zksolc_current_version} which is still not supported and may lead to unexpected errors. Specifying an unsupported version is deprecated and will return an error in future versions of foundry-zksync. Latest version supported is v{zksolc_latest_supported_version}")?;
            }
            Report::new(SpinnerReporter::spawn_with(format!(
                "Using zksolc-{zksolc_current_version}"
            )));
        }

        self.zksync_compile_with(|| {
            let files_to_compile =
                if !files.is_empty() { files } else { project.paths.input_files() };
            let sources = Source::read_all(files_to_compile)?;
            foundry_compilers::project::ProjectCompiler::with_sources(project, sources)?
                .compile()
                .map_err(Into::into)
        })
    }

    #[instrument(target = "forge::compile", skip_all)]
    fn zksync_compile_with<F>(
        self,
        f: F,
    ) -> Result<ProjectCompileOutput<ZkSolcCompiler, ZkArtifactOutput>>
    where
        F: FnOnce() -> Result<ProjectCompileOutput<ZkSolcCompiler, ZkArtifactOutput>>,
    {
        let quiet = self.quiet.unwrap_or(false);
        let bail = self.bail.unwrap_or(true);
        #[allow(clippy::collapsible_else_if)]
        let reporter = if quiet {
            Report::new(NoReporter::default())
        } else {
            if std::io::stdout().is_terminal() {
                Report::new(SpinnerReporter::spawn_with("Compiling (zksync)"))
            } else {
                Report::new(BasicStdoutReporter::default())
            }
        };

        let output = foundry_compilers::report::with_scoped(&reporter, || {
            tracing::debug!("compiling project");

            let timer = std::time::Instant::now();
            let r = f();
            let elapsed = timer.elapsed();

            tracing::debug!("finished compiling in {:.3}s", elapsed.as_secs_f64());
            r
        })?;

        // need to drop the reporter here, so that the spinner terminates
        drop(reporter);

        if bail && output.has_compiler_errors() {
            eyre::bail!("{output}")
        }

        if !quiet {
            if output.is_unchanged() {
                sh_println!("No files changed, compilation skipped")?;
            } else {
                // print the compiler output / warnings
                sh_println!("{output}")?;
            }

            self.zksync_handle_output(&output)?;
        }

        Ok(output)
    }

    /// If configured, this will print sizes or names
    fn zksync_handle_output(
        &self,
        output: &ProjectCompileOutput<ZkSolcCompiler, ZkArtifactOutput>,
    ) -> Result<()> {
        let print_names = self.print_names.unwrap_or(false);
        let print_sizes = self.print_sizes.unwrap_or(false);

        // print any sizes or names
        if print_names {
            let mut artifacts: BTreeMap<_, Vec<_>> = BTreeMap::new();
            for (name, (_, version)) in output.versioned_artifacts() {
                artifacts.entry(version).or_default().push(name);
            }
            for (version, names) in artifacts {
                let _ = sh_println!(
                    "  compiler version: {}.{}.{}",
                    version.major,
                    version.minor,
                    version.patch
                );
                for name in names {
                    let _ = sh_println!("    - {name}");
                }
            }
        }

        if print_sizes {
            // add extra newline if names were already printed
            if print_names && !shell::is_json() {
                let _ = sh_println!();
            }

            let mut size_report = SizeReport {
                report_kind: report_kind(),
                contracts: BTreeMap::new(),
                zksync: self.zksync,
            };

            let artifacts: BTreeMap<_, _> = output
                .artifact_ids()
                .filter(|(id, _)| {
                    // filter out forge-std specific contracts
                    !id.source.to_string_lossy().contains("/forge-std/src/")
                })
                .map(|(id, artifact)| (id.name, artifact))
                .collect();

            for (name, artifact) in artifacts {
                let runtime_size = contract_size(artifact, false).unwrap_or_default();
                let init_size = contract_size(artifact, true).unwrap_or_default();

                let is_dev_contract = artifact
                    .abi
                    .as_ref()
                    .map(|abi| {
                        abi.functions().any(|f| {
                            f.test_function_kind().is_known() ||
                                matches!(f.name.as_str(), "IS_TEST" | "IS_SCRIPT")
                        })
                    })
                    .unwrap_or(false);
                size_report
                    .contracts
                    .insert(name, ContractInfo { runtime_size, init_size, is_dev_contract });
            }

            let _ = sh_println!("{size_report}");

            // TODO: avoid process::exit
            // exit with error if any contract exceeds the size limit, excluding test contracts.
            if size_report.exceeds_runtime_size_limit() {
                std::process::exit(1);
            }

            // Check size limits only if not ignoring EIP-3860
            if !self.ignore_eip_3860 && size_report.exceeds_initcode_size_limit() {
                std::process::exit(1);
            }
        }
        Ok(())
    }
}

impl SizeReport {
    pub(super) fn zk_limits_table_format(
        contract: &ContractInfo,
    ) -> ((isize, Color), (isize, Color)) {
        let runtime_margin = ZKSYNC_CONTRACT_SIZE_LIMIT as isize - contract.runtime_size as isize;
        let init_margin = ZKSYNC_CONTRACT_SIZE_LIMIT as isize - contract.init_size as isize;

        let runtime_color = match contract.runtime_size {
            0..=329999 => Color::Reset,
            330000..=ZKSYNC_CONTRACT_SIZE_LIMIT => Color::Yellow,
            _ => Color::Red,
        };

        let init_color = match contract.init_size {
            0..=329999 => Color::Reset,
            330000..=ZKSYNC_CONTRACT_SIZE_LIMIT => Color::Yellow,
            _ => Color::Red,
        };

        ((runtime_margin, runtime_color), (init_margin, init_color))
    }
}
