//! Support for compiling [foundry_compilers::Project]

use crate::{term::SpinnerReporter, TestFunctionExt};
use comfy_table::{presets::ASCII_MARKDOWN, Attribute, Cell, CellAlignment, Color, Table};
use eyre::Result;
use foundry_block_explorers::contract::Metadata;
use foundry_compilers::{
    artifacts::{remappings::Remapping, BytecodeObject, Source},
    compilers::{
        solc::{Solc, SolcCompiler},
        Compiler,
    },
    report::{BasicStdoutReporter, NoReporter, Report},
    solc::SolcSettings,
    zksolc::{ZkSolc, ZkSolcCompiler},
    zksync::{
        artifact_output::zk::ZkArtifactOutput,
        compile::output::ProjectCompileOutput as ZkProjectCompileOutput,
    },
    Artifact, Project, ProjectBuilder, ProjectCompileOutput, ProjectPathsConfig, SolcConfig,
};
use foundry_zksync_compiler::libraries::{self, ZkMissingLibrary};
use num_format::{Locale, ToFormattedString};
use std::{
    collections::{BTreeMap, HashSet},
    fmt::Display,
    io::IsTerminal,
    path::{Path, PathBuf},
    time::Instant,
};

/// Builder type to configure how to compile a project.
///
/// This is merely a wrapper for [`Project::compile()`] which also prints to stdout depending on its
/// settings.
#[must_use = "ProjectCompiler does nothing unless you call a `compile*` method"]
pub struct ProjectCompiler {
    /// Whether we are going to verify the contracts after compilation.
    verify: Option<bool>,

    /// Whether to also print contract names.
    print_names: Option<bool>,

    /// Whether to also print contract sizes.
    print_sizes: Option<bool>,

    /// Whether to print anything at all. Overrides other `print` options.
    quiet: Option<bool>,

    /// Whether to bail on compiler errors.
    bail: Option<bool>,

    /// Whether to ignore the contract initcode size limit introduced by EIP-3860.
    ignore_eip_3860: bool,

    /// Extra files to include, that are not necessarily in the project's source dir.
    files: Vec<PathBuf>,

    /// Set zksync specific settings based on context
    zksync: bool,
}

impl Default for ProjectCompiler {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectCompiler {
    /// Create a new builder with the default settings.
    #[inline]
    pub fn new() -> Self {
        Self {
            verify: None,
            print_names: None,
            print_sizes: None,
            quiet: Some(crate::shell::is_quiet()),
            bail: None,
            ignore_eip_3860: false,
            files: Vec::new(),
            zksync: false,
        }
    }

    /// Sets whether we are going to verify the contracts after compilation.
    #[inline]
    pub fn verify(mut self, yes: bool) -> Self {
        self.verify = Some(yes);
        self
    }

    /// Sets whether to print contract names.
    #[inline]
    pub fn print_names(mut self, yes: bool) -> Self {
        self.print_names = Some(yes);
        self
    }

    /// Sets whether to print contract sizes.
    #[inline]
    pub fn print_sizes(mut self, yes: bool) -> Self {
        self.print_sizes = Some(yes);
        self
    }

    /// Sets whether to print anything at all. Overrides other `print` options.
    #[inline]
    #[doc(alias = "silent")]
    pub fn quiet(mut self, yes: bool) -> Self {
        self.quiet = Some(yes);
        self
    }

    /// Sets whether to bail on compiler errors.
    #[inline]
    pub fn bail(mut self, yes: bool) -> Self {
        self.bail = Some(yes);
        self
    }

    /// Sets whether to ignore EIP-3860 initcode size limits.
    #[inline]
    pub fn ignore_eip_3860(mut self, yes: bool) -> Self {
        self.ignore_eip_3860 = yes;
        self
    }

    /// Sets extra files to include, that are not necessarily in the project's source dir.
    #[inline]
    pub fn files(mut self, files: impl IntoIterator<Item = PathBuf>) -> Self {
        self.files.extend(files);
        self
    }

    /// Enables zksync contract sizes.
    #[inline]
    pub fn zksync_sizes(mut self) -> Self {
        self.zksync = true;
        self
    }

    /// Compiles the project.
    pub fn compile<C: Compiler>(mut self, project: &Project<C>) -> Result<ProjectCompileOutput<C>> {
        // TODO: Avoid process::exit
        if !project.paths.has_input_files() && self.files.is_empty() {
            println!("Nothing to compile");
            // nothing to do here
            std::process::exit(0);
        }

        // Taking is fine since we don't need these in `compile_with`.
        let files = std::mem::take(&mut self.files);
        self.compile_with(|| {
            let sources = if !files.is_empty() {
                Source::read_all(files)?
            } else {
                project.paths.read_input_files()?
            };

            foundry_compilers::project::ProjectCompiler::with_sources(project, sources)?
                .compile()
                .map_err(Into::into)
        })
    }

    /// Compiles the project with the given closure
    ///
    /// # Example
    ///
    /// ```ignore
    /// use foundry_common::compile::ProjectCompiler;
    /// let config = foundry_config::Config::load();
    /// let prj = config.project().unwrap();
    /// ProjectCompiler::new().compile_with(|| Ok(prj.compile()?)).unwrap();
    /// ```
    #[instrument(target = "forge::compile", skip_all)]
    fn compile_with<C: Compiler, F>(self, f: F) -> Result<ProjectCompileOutput<C>>
    where
        F: FnOnce() -> Result<ProjectCompileOutput<C>>,
    {
        let quiet = self.quiet.unwrap_or(false);
        let bail = self.bail.unwrap_or(true);

        let output = with_compilation_reporter(self.quiet.unwrap_or(false), || {
            tracing::debug!("compiling project");

            let timer = Instant::now();
            let r = f();
            let elapsed = timer.elapsed();

            tracing::debug!("finished compiling in {:.3}s", elapsed.as_secs_f64());
            r
        })?;

        if bail && output.has_compiler_errors() {
            eyre::bail!("{output}")
        }

        if !quiet {
            if output.is_unchanged() {
                println!("No files changed, compilation skipped");
            } else {
                // print the compiler output / warnings
                println!("{output}");
            }

            self.handle_output(&output);
        }

        Ok(output)
    }

    /// If configured, this will print sizes or names
    fn handle_output<C: Compiler>(&self, output: &ProjectCompileOutput<C>) {
        let print_names = self.print_names.unwrap_or(false);
        let print_sizes = self.print_sizes.unwrap_or(false);

        // print any sizes or names
        if print_names {
            let mut artifacts: BTreeMap<_, Vec<_>> = BTreeMap::new();
            for (name, (_, version)) in output.versioned_artifacts() {
                artifacts.entry(version).or_default().push(name);
            }
            for (version, names) in artifacts {
                println!(
                    "  compiler version: {}.{}.{}",
                    version.major, version.minor, version.patch
                );
                for name in names {
                    println!("    - {name}");
                }
            }
        }

        if print_sizes {
            // add extra newline if names were already printed
            if print_names {
                println!();
            }

            let mut size_report = SizeReport { contracts: BTreeMap::new(), zksync: self.zksync };

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

                let dev_functions =
                    artifact.abi.as_ref().map(|abi| abi.functions()).into_iter().flatten().filter(
                        |func| {
                            func.name.is_any_test() ||
                                func.name.eq("IS_TEST") ||
                                func.name.eq("IS_SCRIPT")
                        },
                    );

                let is_dev_contract = dev_functions.count() > 0;
                size_report
                    .contracts
                    .insert(name, ContractInfo { runtime_size, init_size, is_dev_contract });
            }

            println!("{size_report}");

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
    }

    /// Compiles the project.
    pub fn zksync_compile(
        self,
        project: &Project<ZkSolcCompiler, ZkArtifactOutput>,
    ) -> Result<ZkProjectCompileOutput> {
        // TODO: Avoid process::exit
        if !project.paths.has_input_files() && self.files.is_empty() {
            println!("Nothing to compile");
            // nothing to do here
            std::process::exit(0);
        }

        // Taking is fine since we don't need these in `compile_with`.
        //let filter = std::mem::take(&mut self.filter);

        // We need to clone files since we use them in `compile_with`
        // for filtering artifacts in missing libraries detection
        let files = self.files.clone();

        {
            let zksolc_version = ZkSolc::get_version_for_path(&project.compiler.zksolc)?;
            Report::new(SpinnerReporter::spawn_with(format!("Using zksolc-{zksolc_version}")));
        }
        self.zksync_compile_with(&project.paths.root, || {
            let files_to_compile =
                if !files.is_empty() { files } else { project.paths.input_files() };
            let sources = Source::read_all(files_to_compile)?;
            foundry_compilers::zksync::compile::project::ProjectCompiler::with_sources(
                project, sources,
            )?
            .compile()
            .map_err(Into::into)
        })
    }

    #[instrument(target = "forge::compile", skip_all)]
    fn zksync_compile_with<F>(
        self,
        root_path: impl AsRef<Path>,
        f: F,
    ) -> Result<ZkProjectCompileOutput>
    where
        F: FnOnce() -> Result<ZkProjectCompileOutput>,
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
                println!("No files changed, compilation skipped");
            } else {
                // print the compiler output / warnings
                println!("{output}");
            }

            self.zksync_handle_output(root_path, &output)?;
        }

        Ok(output)
    }

    /// If configured, this will print sizes or names
    fn zksync_handle_output(
        &self,
        root_path: impl AsRef<Path>,
        output: &ZkProjectCompileOutput,
    ) -> Result<()> {
        let print_names = self.print_names.unwrap_or(false);
        let print_sizes = self.print_sizes.unwrap_or(false);

        // Process missing libraries
        // TODO: skip this if project was not compiled using --detect-missing-libraries
        let mut missing_libs_unique: HashSet<String> = HashSet::new();
        for (artifact_id, artifact) in output.artifact_ids() {
            // TODO: when compiling specific files, the output might still add cached artifacts
            // that are not part of the file list to the output, which may cause missing libraries
            // error to trigger for files that were not intended to be compiled.
            // This behaviour needs to be investigated better on the foundry-compilers side.
            // For now we filter, checking only the files passed to compile.
            let is_target_file =
                self.files.is_empty() || self.files.iter().any(|f| artifact_id.path == *f);
            if is_target_file {
                if let Some(mls) = artifact.missing_libraries() {
                    missing_libs_unique.extend(mls.clone());
                }
            }
        }

        let missing_libs: Vec<ZkMissingLibrary> = missing_libs_unique
            .into_iter()
            .map(|ml| {
                let mut split = ml.split(':');
                let contract_path =
                    split.next().expect("Failed to extract contract path for missing library");
                let contract_name =
                    split.next().expect("Failed to extract contract name for missing library");

                let mut abs_path_buf = PathBuf::new();
                abs_path_buf.push(root_path.as_ref());
                abs_path_buf.push(contract_path);

                let art = output.find(abs_path_buf.as_path(), contract_name).unwrap_or_else(|| {
                    panic!(
                        "Could not find contract {contract_name} at path {contract_path} for compilation output"
                    )
                });

                ZkMissingLibrary {
                    contract_path: contract_path.to_string(),
                    contract_name: contract_name.to_string(),
                    missing_libraries: art.missing_libraries().cloned().unwrap_or_default(),
                }
            })
            .collect();

        if !missing_libs.is_empty() {
            libraries::add_dependencies_to_missing_libraries_cache(
                root_path,
                missing_libs.as_slice(),
            )
            .expect("Error while adding missing libraries");
            let missing_libs_list = missing_libs
                .iter()
                .map(|ml| format!("{}:{}", ml.contract_path, ml.contract_name))
                .collect::<Vec<String>>()
                .join(", ");
            eyre::bail!("Missing libraries detected: {missing_libs_list}\n\nRun the following command in order to deploy each missing library:\n\nforge create <LIBRARY> --private-key <PRIVATE_KEY> --rpc-url <RPC_URL> --chain <CHAIN_ID> --zksync\n\nThen pass the library addresses using the --libraries option");
        }

        // print any sizes or names
        if print_names {
            let mut artifacts: BTreeMap<_, Vec<_>> = BTreeMap::new();
            for (name, (_, version)) in output.versioned_artifacts() {
                artifacts.entry(version).or_default().push(name);
            }
            for (version, names) in artifacts {
                println!(
                    "  compiler version: {}.{}.{}",
                    version.major, version.minor, version.patch
                );
                for name in names {
                    println!("    - {name}");
                }
            }
        }

        if print_sizes {
            // add extra newline if names were already printed
            if print_names {
                println!();
            }

            let mut size_report = SizeReport { contracts: BTreeMap::new(), zksync: self.zksync };

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

            println!("{size_report}");

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

// https://eips.ethereum.org/EIPS/eip-170
const CONTRACT_RUNTIME_SIZE_LIMIT: usize = 24576;

// https://eips.ethereum.org/EIPS/eip-3860
const CONTRACT_INITCODE_SIZE_LIMIT: usize = 49152;

// https://docs.zksync.io/build/developer-reference/ethereum-differences/contract-deployment#contract-size-limit-and-format-of-bytecode-hash
const ZKSYNC_CONTRACT_SIZE_LIMIT: usize = 450999;

/// Contracts with info about their size
pub struct SizeReport {
    /// `contract name -> info`
    pub contracts: BTreeMap<String, ContractInfo>,
    /// Using zksync size report
    pub zksync: bool,
}

impl SizeReport {
    /// Returns the maximum runtime code size, excluding dev contracts.
    pub fn max_runtime_size(&self) -> usize {
        self.contracts
            .values()
            .filter(|c| !c.is_dev_contract)
            .map(|c| c.runtime_size)
            .max()
            .unwrap_or(0)
    }

    /// Returns the maximum initcode size, excluding dev contracts.
    pub fn max_init_size(&self) -> usize {
        self.contracts
            .values()
            .filter(|c| !c.is_dev_contract)
            .map(|c| c.init_size)
            .max()
            .unwrap_or(0)
    }

    /// Returns true if any contract exceeds the runtime size limit, excluding dev contracts.
    pub fn exceeds_runtime_size_limit(&self) -> bool {
        if self.zksync {
            self.max_runtime_size() > ZKSYNC_CONTRACT_SIZE_LIMIT
        } else {
            self.max_runtime_size() > CONTRACT_RUNTIME_SIZE_LIMIT
        }
    }

    /// Returns true if any contract exceeds the initcode size limit, excluding dev contracts.
    pub fn exceeds_initcode_size_limit(&self) -> bool {
        if self.zksync {
            self.max_init_size() > ZKSYNC_CONTRACT_SIZE_LIMIT
        } else {
            self.max_init_size() > CONTRACT_INITCODE_SIZE_LIMIT
        }
    }
}

impl Display for SizeReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let mut table = Table::new();
        table.load_preset(ASCII_MARKDOWN);
        table.set_header([
            Cell::new("Contract").add_attribute(Attribute::Bold).fg(Color::Blue),
            Cell::new("Runtime Size (B)").add_attribute(Attribute::Bold).fg(Color::Blue),
            Cell::new("Initcode Size (B)").add_attribute(Attribute::Bold).fg(Color::Blue),
            Cell::new("Runtime Margin (B)").add_attribute(Attribute::Bold).fg(Color::Blue),
            Cell::new("Initcode Margin (B)").add_attribute(Attribute::Bold).fg(Color::Blue),
        ]);

        // Filters out dev contracts (Test or Script)
        let contracts = self
            .contracts
            .iter()
            .filter(|(_, c)| !c.is_dev_contract && (c.runtime_size > 0 || c.init_size > 0));
        for (name, contract) in contracts {
            let ((runtime_margin, runtime_color), (init_margin, init_color)) = if self.zksync {
                let runtime_margin =
                    ZKSYNC_CONTRACT_SIZE_LIMIT as isize - contract.runtime_size as isize;
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
            } else {
                let runtime_margin =
                    CONTRACT_RUNTIME_SIZE_LIMIT as isize - contract.runtime_size as isize;
                let init_margin =
                    CONTRACT_INITCODE_SIZE_LIMIT as isize - contract.init_size as isize;

                let runtime_color = match contract.runtime_size {
                    ..18_000 => Color::Reset,
                    18_000..=CONTRACT_RUNTIME_SIZE_LIMIT => Color::Yellow,
                    _ => Color::Red,
                };

                let init_color = match contract.init_size {
                    ..36_000 => Color::Reset,
                    36_000..=CONTRACT_INITCODE_SIZE_LIMIT => Color::Yellow,
                    _ => Color::Red,
                };

                ((runtime_margin, runtime_color), (init_margin, init_color))
            };

            let locale = &Locale::en;
            table.add_row([
                Cell::new(name).fg(Color::Blue),
                Cell::new(contract.runtime_size.to_formatted_string(locale))
                    .set_alignment(CellAlignment::Right)
                    .fg(runtime_color),
                Cell::new(contract.init_size.to_formatted_string(locale))
                    .set_alignment(CellAlignment::Right)
                    .fg(init_color),
                Cell::new(runtime_margin.to_formatted_string(locale))
                    .set_alignment(CellAlignment::Right)
                    .fg(runtime_color),
                Cell::new(init_margin.to_formatted_string(locale))
                    .set_alignment(CellAlignment::Right)
                    .fg(init_color),
            ]);
        }

        writeln!(f, "{table}")?;
        Ok(())
    }
}

/// Returns the deployed or init size of the contract.
fn contract_size<T: Artifact>(artifact: &T, initcode: bool) -> Option<usize> {
    let bytecode = if initcode {
        artifact.get_bytecode_object()?
    } else {
        artifact.get_deployed_bytecode_object()?
    };

    let size = match bytecode.as_ref() {
        BytecodeObject::Bytecode(bytes) => bytes.len(),
        BytecodeObject::Unlinked(unlinked) => {
            // we don't need to account for placeholders here, because library placeholders take up
            // 40 characters: `__$<library hash>$__` which is the same as a 20byte address in hex.
            let mut size = unlinked.as_bytes().len();
            if unlinked.starts_with("0x") {
                size -= 2;
            }
            // hex -> bytes
            size / 2
        }
    };

    Some(size)
}

/// How big the contract is and whether it is a dev contract where size limits can be neglected
#[derive(Clone, Copy, Debug)]
pub struct ContractInfo {
    /// Size of the runtime code in bytes
    pub runtime_size: usize,
    /// Size of the initcode in bytes
    pub init_size: usize,
    /// A development contract is either a Script or a Test contract.
    pub is_dev_contract: bool,
}

/// Compiles target file path.
///
/// If `quiet` no solc related output will be emitted to stdout.
///
/// If `verify` and it's a standalone script, throw error. Only allowed for projects.
///
/// **Note:** this expects the `target_path` to be absolute
pub fn compile_target<C: Compiler>(
    target_path: &Path,
    project: &Project<C>,
    quiet: bool,
) -> Result<ProjectCompileOutput<C>> {
    ProjectCompiler::new().quiet(quiet).files([target_path.into()]).compile(project)
}

/// Creates a [Project] from an Etherscan source.
pub fn etherscan_project(
    metadata: &Metadata,
    target_path: impl AsRef<Path>,
) -> Result<Project<SolcCompiler>> {
    let target_path = dunce::canonicalize(target_path.as_ref())?;
    let sources_path = target_path.join(&metadata.contract_name);
    metadata.source_tree().write_to(&target_path)?;

    let mut settings = metadata.settings()?;

    // make remappings absolute with our root
    for remapping in settings.remappings.iter_mut() {
        let new_path = sources_path.join(remapping.path.trim_start_matches('/'));
        remapping.path = new_path.display().to_string();
    }

    // add missing remappings
    if !settings.remappings.iter().any(|remapping| remapping.name.starts_with("@openzeppelin/")) {
        let oz = Remapping {
            context: None,
            name: "@openzeppelin/".into(),
            path: sources_path.join("@openzeppelin").display().to_string(),
        };
        settings.remappings.push(oz);
    }

    // root/
    //   ContractName/
    //     [source code]
    let paths = ProjectPathsConfig::builder()
        .sources(sources_path.clone())
        .remappings(settings.remappings.clone())
        .build_with_root(sources_path);

    let v = metadata.compiler_version()?;
    let solc = Solc::find_or_install(&v)?;

    let compiler = SolcCompiler::Specific(solc);

    Ok(ProjectBuilder::<SolcCompiler>::default()
        .settings(SolcSettings {
            settings: SolcConfig::builder().settings(settings).build(),
            ..Default::default()
        })
        .paths(paths)
        .ephemeral()
        .no_artifacts()
        .build(compiler)?)
}

/// Configures the reporter and runs the given closure.
pub fn with_compilation_reporter<O>(quiet: bool, f: impl FnOnce() -> O) -> O {
    #[allow(clippy::collapsible_else_if)]
    let reporter = if quiet {
        Report::new(NoReporter::default())
    } else {
        if std::io::stdout().is_terminal() {
            Report::new(SpinnerReporter::spawn())
        } else {
            Report::new(BasicStdoutReporter::default())
        }
    };

    foundry_compilers::report::with_scoped(&reporter, f)
}
