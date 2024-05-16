#![allow(missing_docs)]
//! This module provides the implementation of the ZkSolc compiler for Solidity contracts.
use crate::{
    libraries,
    zksolc::config::{Settings, ZkSolcConfig, ZkStandardJsonCompilerInput},
};
/// ZkSolc is a specialized compiler that supports zero-knowledge (ZK) proofs for smart
/// contracts.
///
/// The `ZkSolc` struct represents an instance of the compiler, and it is responsible for
/// compiling Solidity contracts and handling the output. It uses the `solc` library to
/// interact with the Solidity compiler.
///
/// The `ZkSolc` struct provides the following functionality:
///
/// - Configuration: It allows configuring the compiler path, system mode, and force-evmla
///   options through the `ZkSolcOpts` struct.
///
/// - Compilation: The `compile` method initiates the compilation process. It collects the
///   source files, parses the JSON input, builds compiler arguments, runs the compiler, and
///   handles the output.
///
/// - Error and Warning Handling: The compiler output is checked for errors and warnings, and
///   they are displayed appropriately. If errors are encountered, the process will exit with a
///   non-zero status code.
///
/// - JSON Input Generation: The `parse_json_input` method generates the JSON input required by
///   the compiler for each contract. It configures the Solidity compiler, saves the input to
///   the artifacts directory, and handles the output.
///
/// - Source Management: The `get_versioned_sources` method retrieves the project sources,
///   resolves the graph of sources and versions, and returns the sources grouped by Solc
///   version.
///
/// - Artifact Path Generation: The `build_artifacts_path` and `build_artifacts_file` methods
///   construct the path and file for saving the compiler output artifacts.
use alloy_json_abi::JsonAbi;
use alloy_primitives::Bytes;
use ansi_term::Colour::{Red, Yellow};
use eyre::{Context, Result};
use foundry_compilers::{
    artifacts::{
        output_selection::FileOutputSelection, CompactBytecode, CompactDeployedBytecode,
        FileToContractsMap, Source, StandardJsonCompilerInput,
    },
    ArtifactFile, Artifacts, ConfigurableContractArtifact, Graph, Project, ProjectCompileOutput,
    Solc,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
    fs::{self, File},
    io::{BufWriter, Read},
    path::{Path, PathBuf},
    process::{exit, Command, Stdio},
};
use tracing::{error, info, trace, warn};

use crate::zksolc::PackedEraBytecode;

/// It is observed that when there's a missing library without
/// `--detect-missing-libraries` an error is thrown that contains
/// this message fragment
const MISSING_LIBS_ERROR: &[u8] = b"not found in the project".as_slice();

/// Mapping of bytecode hash (without "0x" prefix) to the respective contract name.
pub type ContractBytecodes = BTreeMap<String, String>;

#[derive(Debug, Clone)]
pub struct ZkSolcOpts {
    pub compiler_path: PathBuf,
    pub is_system: bool,
    pub force_evmla: bool,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CompilerError {
    component: String,
    #[serde(rename = "errorCode")]
    error_code: Option<String>,
    #[serde(rename = "formattedMessage")]
    formatted_message: String,
    message: String,
    severity: String,
    #[serde(rename = "sourceLocation")]
    source_location: SourceLocation,
    #[serde(rename = "type")]
    type_of_error: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SourceLocation {
    file: String,
    start: u32,
    end: u32,
}

/// Files that should be compiled with a given solidity version.
type SolidityVersionSources = (Version, BTreeMap<PathBuf, Source>);

/// This struct represents the ZkSolc compiler for compiling Solidity contracts.
///
/// Key Components:
/// - Manages project details, including paths and configurations.
/// - Stores the compiler path.
/// - Provides a comprehensive interface for compiling Solidity contracts using the ZkSolc compiler.
///
/// Struct Members:
/// - `project`: Represents the project details and configurations.
/// - `compiler_path`: The path to the ZkSolc compiler executable.
/// - `is_system`: A flag indicating whether the compiler is in system mode.
/// - `force_evmla`: A flag indicating whether to force EVMLA optimization.
/// - `standard_json`: An optional field to store the parsed standard JSON input for the contracts.
/// - `sources`: An optional field to store the versioned sources for the contracts.
///
/// Functionality:
/// - `new`: Constructs a new `ZkSolc` instance using the provided compiler path, project
///   configurations, and options.
/// - `compile`: Responsible for compiling the contracts in the project's 'sources' directory and
///   its subdirectories.
///
/// Error Handling:
/// - The methods in this struct return the `Result` type from the `anyhow` crate for flexible and
///   easy-to-use error handling.
///
/// Example Usage:
/// ```ignore
/// use zk_solc::{ZkSolc};
/// use ethers_solc::Project;
/// // Set the compiler path and other options
/// let compiler_path = "/path/to/zksolc";
///
/// let project = Project::builder().build().unwrap();
///
/// // Initialize the ZkSolc compiler
/// let zksolc = ZkSolc::new(compiler_path, project);
///
/// // Compile the contracts
/// if let Err(err) = zksolc.compile() {
///     eprintln!("Failed to compile contracts: {}", err);
///     // Handle the error
/// }
/// ```
///
/// In this example, the `ZkSolc` compiler is initialized with the provided compiler path and
/// project configurations. The `compile` method is then invoked to compile the contracts, and any
/// resulting errors are handled accordingly.
#[derive(Debug)]
pub struct ZkSolc {
    config: ZkSolcConfig,
    project: Project,
    standard_json: Option<ZkStandardJsonCompilerInput>,
}

impl fmt::Display for ZkSolc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ZkSolc (
                compiler_path: {},
                output_path: {},
            )",
            self.config.compiler_path.display(),
            self.project.paths.artifacts.display(),
        )
    }
}

/// The `compile_smart_contracts` function initiates the contract compilation process.
///
/// It follows these steps:
/// 1. Create an instance of `ZkSolcOpts` with the appropriate options.
/// 2. Instantiate `ZkSolc` with the created options and the project.
/// 3. Initiate the contract compilation process.
///
/// The function returns `Ok(())` if the compilation process completes successfully, or an error
/// if it fails.
pub fn compile_smart_contracts(
    zksolc_cfg: ZkSolcConfig,
    project: Project,
) -> eyre::Result<ProjectCompileOutput> {
    //TODO: use remappings
    let mut zksolc = ZkSolc::new(zksolc_cfg, project);
    match zksolc.compile() {
        Ok(output) => {
            info!("Compiled Successfully");
            Ok(output)
        }
        Err(err) => {
            eyre::bail!("Failed to compile smart contracts with zksolc: {}", err);
        }
    }
}

/// Represents a cache hit or a miss for a contract.
pub enum CachedContractEntry {
    /// Cache hit.
    Found { cache: ContractCache, output: ZkSolcCompilerOutput },
    /// Cache miss.
    Missing { cache: ContractCache },
}

/// Represents the state of a cached contract.
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct ContractCache {
    /// The base cache directory for the contract.
    #[serde(skip)]
    pub base_path: PathBuf,
    /// The cached metadata file for the contract.
    #[serde(skip)]
    pub metadata_path: PathBuf,
    /// The cached output file for the contract.
    #[serde(skip)]
    pub output_path: PathBuf,
    /// Provided input file.
    pub input_file: String,
    /// Hash of the input file's contents.
    pub hash: String,
}

impl ContractCache {
    /// Creates a new cache instance for the given contract.
    /// The function can fail on reading the input file, and/or calculating its content hash.
    /// The contract hash is based on xxh3 hash of its relative path to the root.
    pub fn new(root_path: &Path, cache_path: &Path, contract_path: &Path) -> Result<Self> {
        let contract_hash = File::open(contract_path)
            .and_then(|mut file| {
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer).map(|_| buffer)
            })
            .map(|data| hex::encode(xxhash_rust::const_xxh3::xxh3_64(&data).to_be_bytes()))
            .map_err(|err| eyre::eyre!("failed hashing input contract: {err:?}"))?;

        let relative_contract_path = contract_path.strip_prefix(root_path).unwrap_or(contract_path);
        let input_file =
            relative_contract_path.to_str().expect("failed creating contract_path").to_string();
        let contract_path_hash =
            hex::encode(xxhash_rust::const_xxh3::xxh3_64(input_file.as_bytes()).to_be_bytes());

        let base_path = cache_path.join(contract_path_hash);
        if !base_path.exists() {
            fs::create_dir_all(&base_path).map_err(|err| {
                eyre::eyre!("Could not create contract cache path: {base_path:?} - {err:?}",)
            })?;
        }

        let output_path = base_path.join("output.json");
        let metadata_path = base_path.join("metadata.json");

        Ok(ContractCache { base_path, metadata_path, output_path, input_file, hash: contract_hash })
    }

    /// Writes the contract's output into the cache along with the metadata.
    pub fn write(&self, output: &ZkSolcCompilerOutput) {
        let file = File::create(&self.output_path).unwrap();
        let writer = std::io::BufWriter::new(file);
        serde_json::to_writer(writer, &output).unwrap();

        let output_file = File::create(&self.metadata_path)
            .wrap_err("Could not create contract cache file")
            .unwrap();
        serde_json::to_writer(BufWriter::new(output_file), &self)
            .unwrap_or_else(|e| panic!("Could not write contract cache file: {}", e));
    }

    /// Reads the cached output if the input file content hash matches the stored entry.
    pub fn read_cached_output(&self) -> Result<ZkSolcCompilerOutput> {
        if !self.metadata_path.exists() {
            eyre::bail!("{:?} does not exist", self.metadata_path);
        }
        if !self.output_path.exists() {
            eyre::bail!("{:?} does not exist", self.output_path);
        }

        // https://docs.rs/serde_json/latest/serde_json/fn.from_reader.html
        // serde_json docs recommend buffering the whole reader to a string
        // This also prevents a borrowing issue when deserializing from a reader
        File::open(&self.metadata_path)
            .map_err(|err| eyre::eyre!("failed reading metadata: {:?}", err))
            .and_then(|file| {
                let mut reader = std::io::BufReader::new(file);
                let mut buffer = String::with_capacity(1024);
                reader
                    .read_to_string(&mut buffer)
                    .map_err(|err| eyre::eyre!("failed reading output to buffer: {:?}", err))
                    .and_then(|_| {
                        serde_json::from_str::<ContractCache>(&buffer)
                            .map_err(|err| eyre::eyre!("failed deserializing output: {:?}", err))
                    })
            })
            .and_then(|persisted_cache| {
                if self.hash != persisted_cache.hash {
                    Err(eyre::eyre!(
                        "expected input hash {:?}, found persisted hash {:?}",
                        self.hash,
                        persisted_cache.hash
                    ))
                } else {
                    Ok(())
                }
            })?;

        File::open(&self.output_path)
            .map_err(|err| eyre::eyre!("failed reading output: {:?}", err))
            .and_then(|file| {
                let mut reader = std::io::BufReader::new(file);
                let mut buffer = String::with_capacity(1024);
                reader
                    .read_to_string(&mut buffer)
                    .map_err(|err| eyre::eyre!("failed reading output to buffer: {:?}", err))
                    .and_then(|_| {
                        serde_json::from_str::<ZkSolcCompilerOutput>(&buffer)
                            .map_err(|err| eyre::eyre!("failed deserializing output: {:?}", err))
                    })
            })
    }
}

impl ZkSolc {
    pub fn new(config: ZkSolcConfig, project: Project) -> Self {
        Self { config, project, standard_json: None }
    }

    /// Compiles the Solidity contracts in the project's 'sources' directory and its subdirectories
    /// using the ZkSolc compiler.
    ///
    /// # Arguments
    ///
    /// * `self` - A mutable reference to the `ZkSolc` instance.
    ///
    /// # Errors
    ///
    /// This function can return an error if any of the following occurs:
    /// - The Solidity compiler fails to execute or encounters an error during compilation.
    /// - The source files cannot be collected from the project's 'sources' directory.
    /// - The compiler arguments cannot be built.
    /// - The output of the compiler contains errors or warnings.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let project = Project::new(...);
    /// let opts = ZkSolcOpts {
    ///     compiler_path: PathBuf::from("/path/to/zksolc"),
    ///     is_system: false,
    ///     force_evmla: true,
    /// };
    /// let mut zksolc = ZkSolc::new(opts, project);
    /// zksolc.compile()?;
    /// ```
    ///
    /// In this example, a `ZkSolc` instance is created using `ZkSolcOpts` and a `Project`. Then,
    /// the `compile` method is invoked to compile the contracts.
    ///
    /// # Workflow
    ///
    /// The `compile` function performs the following operations:
    ///
    /// 1. Collect Source Files:
    ///    - It collects the source files from the project's 'sources' directory and its
    ///      subdirectories.
    ///    - Only the files within the 'sources' directory and its subdirectories are considered for
    ///      compilation.
    ///
    /// 2. Configure Solidity Compiler:
    ///    - It configures the Solidity compiler by setting options like the compiler path, system
    ///      mode, and force EVMLA flag.
    ///
    /// 3. Parse JSON Input:
    ///    - For each source file, it parses the JSON input using the Solidity compiler.
    ///    - The parsed JSON input is stored in the `standard_json` field of the `ZkSolc` instance.
    ///
    /// 4. Build Compiler Arguments:
    ///    - It builds the compiler arguments for each source file.
    ///    - The compiler arguments include options like the compiler path, system mode, and force
    ///      EVMLA flag.
    ///
    /// 5. Run Compiler and Handle Output:
    ///    - It runs the Solidity compiler for each source file with the corresponding compiler
    ///      arguments.
    ///    - The output of the compiler, including errors and warnings, is captured.
    ///
    /// 6. Handle Output (Errors and Warnings):
    ///    - It handles the output of the compiler, extracting errors and warnings.
    ///    - Errors are printed in red, and warnings are printed in yellow.
    ///
    /// 7. Save Artifacts:
    ///    - It saves the artifacts (compiler output) as a JSON file for each source file.
    ///    - The artifacts are saved in the project's artifacts directory under the corresponding
    ///      source file's directory.
    ///
    /// # Note
    ///
    /// The `compile` function modifies the `ZkSolc` instance to store the parsed JSON input and the
    /// versioned sources. These modified values can be accessed after the compilation process
    /// for further processing or analysis.
    pub fn compile(&mut self) -> Result<ProjectCompileOutput> {
        let mut displayed_warnings = HashSet::new();

        // Step 1: Collect Source Files
        let mut sources = self.get_versioned_sources().wrap_err("Cannot get source files")?;
        sources.retain(|_, (_, sources)| {
            sources.retain(|path, _| {
                let relative_path = path.strip_prefix(self.project.root()).unwrap_or(path.as_ref());
                // prune ignored contracts
                !self.is_contract_ignored_in_config(relative_path)
            });
            // and prune any group that is now empty
            !sources.is_empty()
        });

        // Step 2: Check missing libraries
        // Map from (contract_path, contract_name) -> missing_libraries
        let mut all_missing_libraries: HashMap<(String, String), HashSet<String>> = HashMap::new();

        // Step 3: Proceed with compilation
        let mut all_artifacts = BTreeMap::new();
        for (solc, version) in sources {
            info!(solc = ?solc.solc, "\nCompiling {} files...", version.1.len());
            // Configure project solc for each solc version
            for (contract_path, _) in version.1 {
                let relative_contract_path = contract_path
                    .strip_prefix(&self.project.paths.root)
                    .expect("file must belong to root path");
                let artifacts = match self
                    .check_cache(&self.project.paths.artifacts, &contract_path)?
                {
                    CachedContractEntry::Found { output, .. } => {
                        info!("Using hashed artifact for {:?}", relative_contract_path);
                        ZkSolc::handle_output(
                            &output,
                            relative_contract_path,
                            &mut displayed_warnings,
                        )
                    }
                    CachedContractEntry::Missing { cache } => {
                        self.prepare_compiler_input(&contract_path, &cache.base_path).wrap_err(
                            format!("Failed to prepare inputs when compiling {:?}", contract_path),
                        )?;

                        let Some(output) = self.run_compiler(&contract_path, &solc)? else {
                            continue
                        };

                        let missing_libraries =
                            Self::get_missing_libraries_from_output(&output.stdout)?;
                        tracing::trace!(path = ?contract_path, ?missing_libraries);
                        let has_missing_libraries = !missing_libraries.is_empty();

                        // collect missing libraries
                        for missing_library in missing_libraries {
                            all_missing_libraries
                                .entry((
                                    missing_library.contract_path.clone(),
                                    missing_library.contract_name.clone(),
                                ))
                                .and_modify(|missing_dependencies| {
                                    missing_dependencies
                                        .extend(missing_library.missing_libraries.clone())
                                })
                                .or_insert_with(|| {
                                    HashSet::from_iter(missing_library.missing_libraries.clone())
                                });
                        }

                        if has_missing_libraries {
                            //skip current contract output from processing
                            continue;
                        }

                        // Deserialize the compiler output into a serde_json::Value object
                        let output = output.stdout;
                        let compiler_output =
                            match serde_json::from_slice::<ZkSolcCompilerOutput>(&output) {
                                Ok(output) => output,
                                Err(_) => {
                                    let output_str = String::from_utf8_lossy(&output);
                                    let parsed_json: Result<serde_json::Value, _> =
                                        serde_json::from_str(&output_str);

                                    match parsed_json {
                                        Ok(json) if json.get("errors").is_some() => {
                                            let errors = json["errors"]
                                                .as_array()
                                                .expect("Expected 'errors' to be an array")
                                                .iter()
                                                .map(|e| {
                                                    serde_json::from_value(e.clone())
                                                        .expect("Error parsing error")
                                                })
                                                .collect::<Vec<CompilerError>>();
                                            // Handle errors in the output
                                            ZkSolc::handle_output_errors(errors);
                                        }
                                        _ => info!("Failed to parse compiler output!"),
                                    }
                                    exit(1);
                                }
                            };
                        let artifacts = ZkSolc::handle_output(
                            &compiler_output,
                            relative_contract_path,
                            &mut displayed_warnings,
                        );
                        cache.write(&compiler_output);

                        artifacts
                    }
                };

                let contract_path =
                    contract_path.to_str().expect("failed creating artifact_path str").to_string();
                all_artifacts.insert(contract_path, artifacts);
            }
        }

        // Convert the artifacts to their canonical file paths
        let all_artifacts = self.canonicalize_artifact_paths(all_artifacts);

        // Step 4: If missing library dependencies, save them to a file and return an error
        if !all_missing_libraries.is_empty() {
            let dependencies: Vec<ZkMissingLibrary> = all_missing_libraries
                .into_iter()
                .map(|((contract_path, contract_name), missing_libraries)| ZkMissingLibrary {
                    contract_path,
                    contract_name,
                    missing_libraries: missing_libraries.into_iter().collect(),
                })
                .collect();
            libraries::add_dependencies_to_missing_libraries_cache(
                &self.project.paths.root,
                dependencies.as_slice(),
            )?;
            eyre::bail!("Missing libraries detected {:?}\n\nRun the following command in order to deploy the missing libraries:\nforge create --deploy-missing-libraries --private-key <PRIVATE_KEY> --rpc-url <RPC_URL> --chain <CHAIN_ID> --zksync", dependencies);
        }

        let mut result = ProjectCompileOutput::default();
        result.set_compiled_artifacts(Artifacts(all_artifacts));
        Ok(result)
    }

    /// Converts the artifacts into canonical foundry artifact paths using it's custom
    /// name-resolution scheme. This mirrors the artifact paths generated via solc compilation.
    /// This function takes artifacts indexed as "Foo.sol/Foo.json" and generate a collision-free
    /// path in the artifacts directory.
    fn canonicalize_artifact_paths(
        &self,
        mut all_artifacts: BTreeMap<
            String,
            BTreeMap<String, ArtifactFile<ConfigurableContractArtifact>>,
        >,
    ) -> FileToContractsMap<Vec<ArtifactFile<ConfigurableContractArtifact>>> {
        // Create a mapping of artifacts with their artifact keys. This will group duplicates
        // together.
        let mut mapped_contracts = HashMap::new();
        for (contract_path, artifacts) in &mut all_artifacts {
            for (artifact_key, artifact) in artifacts {
                mapped_contracts
                    .entry(artifact_key)
                    .or_insert(vec![])
                    .push((contract_path, artifact));
            }
        }

        // Generate unique paths per artifact, resolving duplicates
        let mut final_artifact_paths = HashSet::new();
        for (artifact_key, mut contracts) in mapped_contracts {
            let mut contract_artifact_paths = vec![];
            for (idx, (contract_path, _)) in contracts.iter().enumerate() {
                let mut artifact_path =
                    self.project.paths.artifacts.join(PathBuf::from(&artifact_key));
                if contracts.len() > 1 {
                    // naming conflict where the `artifact_path` belongs to two conflicting
                    // contracts need to adjust the paths properly

                    // we keep the top most conflicting file unchanged
                    let is_top_most = contracts.iter().enumerate().filter(|(i, _)| *i != idx).all(
                        |(_, (cp, _))| {
                            Path::new(contract_path).components().count() <
                                Path::new(cp).components().count()
                        },
                    );
                    if !is_top_most {
                        // we resolve the conflicting by finding a new unique, alternative path
                        artifact_path = Self::conflict_free_output_file(
                            &final_artifact_paths,
                            artifact_path,
                            Path::new(&contract_path),
                            &self.project.paths.artifacts,
                        );
                    }
                }
                final_artifact_paths.insert(artifact_path.clone());
                contract_artifact_paths.push(artifact_path);
            }
            for (idx, artifact_path) in contract_artifact_paths.into_iter().enumerate() {
                contracts[idx].1.file = artifact_path
            }
        }

        // Recreate the artifacts with their canonical file path, as generated by foundry's solc
        // compilation
        all_artifacts
            .into_iter()
            .map(|(contract_path, artifacts)| {
                let artifacts = artifacts
                    .into_values()
                    .map(|artifact| {
                        let artifact_path =
                            artifact.file.to_str().expect("must be valid path").to_string();
                        (artifact_path, vec![artifact])
                    })
                    .collect();
                (contract_path, artifacts)
            })
            .collect()
    }

    /// Checks if the contract has already been compiled for the given input contract hash.
    /// If yes, returns the pre-compiled data.
    fn check_cache(&self, cache_path: &Path, contract_path: &Path) -> Result<CachedContractEntry> {
        let cache = ContractCache::new(&self.project.paths.root, cache_path, contract_path)?;
        match cache.read_cached_output() {
            Ok(output) => Ok(CachedContractEntry::Found { cache, output }),
            Err(err) => {
                tracing::trace!("missed cache for {contract_path:?}: {err:?}");
                Ok(CachedContractEntry::Missing { cache })
            }
        }
    }

    /// Builds the compiler arguments for the Solidity compiler based on the provided versioned
    /// source and solc instance. The compiler arguments specify options and settings for the
    /// compiler's execution.
    ///
    /// # Arguments
    ///
    /// * `versioned_source` - A tuple containing the contract source path (`PathBuf`) and the
    ///   corresponding `Source` object.
    /// * `solc` - The `Solc` instance representing the specific version of the Solidity compiler.
    ///
    /// # Returns
    ///
    /// A vector of strings representing the compiler arguments.
    fn build_compiler_args<'s>(
        &'s self,
        contract_path: &'s Path,
        solc: &'s Solc,
        detect_missing_libraries: bool,
    ) -> Vec<&'s str> {
        // Get the solc compiler path as a string
        let solc_path = solc.solc.to_str().expect("Given solc compiler path wasn't valid.");

        // Build compiler arguments
        let mut comp_args = vec!["--standard-json", "--solc", solc_path];

        // Check if system mode is enabled or if the source path contains "is-system"
        if self.config.settings.is_system ||
            contract_path
                .to_str()
                .expect("Given contract path wasn't valid.")
                .contains("is-system")
        {
            comp_args.push("--system-mode");
        }

        // Check if force-evmla is enabled
        if self.config.settings.force_evmla {
            comp_args.push("--force-evmla");
        }

        // Check if should detect missing libraries
        if detect_missing_libraries {
            comp_args.push("--detect-missing-libraries");
        }

        comp_args
    }

    /// Handles the output of the Solidity compiler after the compilation process is completed. It
    /// processes the compiler output, handles errors and warnings, and saves the compiler
    /// artifacts.
    ///
    /// # Arguments
    ///
    /// * `output` - The output of the Solidity compiler as a `std::process::Output` struct.
    /// * `source` - The relative path of the contract source file that was compiled.
    /// * `displayed_warnings` - A mutable set that keeps track of displayed warnings to avoid
    ///   duplicates.
    ///
    /// # Output Handling
    ///
    /// - The output of the Solidity compiler is expected to be in JSON format.
    /// - The output is deserialized into a `serde_json::Value` object for further processing.
    ///
    /// # Error and Warning Handling
    ///
    /// - The function checks for errors and warnings in the compiler output and handles them
    ///   accordingly.
    /// - Errors are printed in red color.
    /// - Warnings are printed in yellow color.
    /// - If an error is encountered, the function exits with a non-zero status code.
    /// - If only warnings are present, a message indicating the presence of warnings is printed.
    ///
    /// # Artifacts Saving
    ///
    /// - The function saves the compiler output (artifacts) in a file.
    /// - The artifacts are saved in a file named "artifacts.json" within the contract's artifacts
    ///   directory.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let output = std::process::Output { ... };
    /// let source = "src/foo/contract.sol".to_string();
    /// let mut displayed_warnings = HashSet::new();
    /// let compiler_output = serde_json::from_slice::<ZkSolcCompilerOutput>(&output).unwrap();
    /// ZkSolc::handle_output(&compiler_output, source, &mut displayed_warnings);
    /// ```
    ///
    /// In this example, the `handle_output` function is called with the compiler output, contract
    /// source, and a mutable set for displayed warnings. It processes the output, handles
    /// errors and warnings, and saves the artifacts.
    pub fn handle_output(
        compiler_output: &ZkSolcCompilerOutput,
        source: &Path,
        displayed_warnings: &mut HashSet<String>,
    ) -> BTreeMap<String, ArtifactFile<ConfigurableContractArtifact>> {
        // Handle warnings in the output
        ZkSolc::handle_output_warnings(compiler_output, displayed_warnings);

        // First - let's get all the bytecodes.
        let mut all_bytecodes: HashMap<String, String> = Default::default();
        for source_file_results in compiler_output.contracts.values() {
            for contract_results in source_file_results.values() {
                if let Some(hash) = &contract_results.hash {
                    all_bytecodes.insert(
                        hash.clone(),
                        contract_results
                            .evm
                            .as_ref()
                            .unwrap()
                            .bytecode
                            .as_ref()
                            .unwrap()
                            .object
                            .clone(),
                    );
                }
            }
        }

        // Get the output entry only for the source
        let source_path = source.to_str().expect("must be valid string");
        let mut result = BTreeMap::new();
        if let Some(contracts_in_file) = compiler_output.contracts.get(source_path) {
            for (contract_name, contract) in contracts_in_file {
                // if contract hash is empty, skip
                if contract.hash.is_none() {
                    trace!("{} -> empty contract.hash", contract_name);
                    continue
                }

                info!("{} -> Bytecode Hash: {} ", contract_name, contract.hash.as_ref().unwrap());

                let factory_deps: Vec<String> = contract
                    .factory_dependencies
                    .as_ref()
                    .unwrap()
                    .keys()
                    .map(|factory_hash| all_bytecodes.get(factory_hash).unwrap())
                    .cloned()
                    .collect();

                let packed_bytecode = Bytes::from(
                    PackedEraBytecode::new(
                        contract.hash.as_ref().unwrap().clone(),
                        contract.evm.as_ref().unwrap().bytecode.as_ref().unwrap().object.clone(),
                        factory_deps,
                    )
                    .to_vec(),
                );

                let mut art = ConfigurableContractArtifact {
                    bytecode: Some(CompactBytecode {
                        object: foundry_compilers::artifacts::BytecodeObject::Bytecode(
                            packed_bytecode.clone(),
                        ),
                        source_map: None,
                        link_references: Default::default(),
                    }),
                    deployed_bytecode: Some(CompactDeployedBytecode {
                        bytecode: Some(CompactBytecode {
                            object: foundry_compilers::artifacts::BytecodeObject::Bytecode(
                                packed_bytecode,
                            ),
                            source_map: None,
                            link_references: Default::default(),
                        }),
                        immutable_references: Default::default(),
                    }),
                    // Initialize other fields with their default values if they exist
                    ..ConfigurableContractArtifact::default()
                };

                art.abi = contract.abi.clone();

                let source_filename = source
                    .file_name()
                    .expect("must have filename")
                    .to_str()
                    .expect("must be valid string");
                let artifact_key = format!("{}/{}.json", source_filename, contract_name);
                let artifact = ArtifactFile {
                    artifact: art,
                    file: PathBuf::from(&artifact_key),
                    version: Version::parse(&compiler_output.version).unwrap(),
                };
                result.insert(artifact_key, artifact);
            }
        }

        result
    }

    /// Handles the errors and warnings present in the output JSON from the compiler.
    ///
    /// # Arguments
    ///
    /// * `output_json` - A reference to the output JSON from the compiler, represented as a `Value`
    ///   from the `serde_json` crate.
    /// * `displayed_warnings` - A mutable reference to a `HashSet` that tracks displayed warnings
    ///   to avoid duplicates.
    ///
    /// # Behavior
    ///
    /// This function iterates over the `errors` array in the output JSON and processes each error
    /// or warning individually. For each error or warning, it extracts the severity and
    /// formatted message from the JSON. If the severity is "warning", it checks if the same
    /// warning message has been displayed before to avoid duplicates. If the warning message
    /// has not been displayed before, it adds the message to the `displayed_warnings` set,
    /// prints the formatted warning message in yellow, and sets the `has_warning` flag to true.
    /// If the severity is not "warning", it prints the formatted error message in red and sets
    /// the `has_error` flag to true.
    ///
    /// If any errors are encountered, the function calls `exit(1)` to terminate the program. If
    /// only warnings are encountered, it prints a message indicating that the compiler run
    /// completed with warnings.
    pub fn handle_output_warnings(
        output_json: &ZkSolcCompilerOutput,
        displayed_warnings: &mut HashSet<String>,
    ) {
        let errors = &output_json.errors;

        let mut has_error = false;
        let mut has_warning = false;

        for error in errors {
            let severity = error.get("severity").and_then(|v| v.as_str()).unwrap_or("Unknown");
            let formatted_message =
                error.get("formattedMessage").and_then(|v| v.as_str()).unwrap_or("");

            let is_warning = severity.eq_ignore_ascii_case("warning");
            if is_warning {
                let main_message = formatted_message.lines().next().unwrap_or("").to_string();
                if !displayed_warnings.contains(&main_message) {
                    displayed_warnings.insert(main_message);
                    println!("{}", Yellow.paint(formatted_message));
                    has_warning = true;
                }
            } else {
                println!("{}", Red.paint(formatted_message));
                has_error = true;
            }
        }

        if has_error {
            exit(1);
        } else if has_warning {
            warn!("Compiler run completed with warnings");
        }
    }
    /// Handles and formats the errors present in the output JSON from the zksolc compiler.
    pub fn handle_output_errors(errors: Vec<CompilerError>) {
        let mut has_error = false;
        let mut error_codes = Vec::new();

        for error in errors {
            if error.severity.eq_ignore_ascii_case("error") {
                let error_message = &error.formatted_message;
                error!("{}", Red.paint(error_message));
                if let Some(code) = &error.error_code {
                    error_codes.push(code.clone());
                }
                has_error = true;
            }
        }

        if has_error {
            for code in error_codes {
                error!("{}", Red.paint(format!("Compilation failed with error code: {}", code)));
            }
            exit(1);
        }
    }

    /// Parses the JSON input for a contract and prepares the necessary configuration for the ZkSolc
    /// compiler.
    ///
    /// # Arguments
    ///
    /// * `contract_path` - The path to the contract source file.
    ///
    /// # Errors
    ///
    /// This function can return an error if any of the following occurs:
    /// - The standard JSON input cannot be generated for the contract.
    /// - The artifacts path for the contract cannot be created.
    /// - The JSON input cannot be saved to the artifacts directory.
    ///
    /// # Workflow
    ///
    /// The `parse_json_input` function performs the following operations:
    ///
    /// 1. Configure File Output Selection:
    ///    - It configures the file output selection to specify which outputs should be included in
    ///      the compiler output.
    ///
    /// 2. Configure Solidity Compiler:
    ///    - It modifies the Solidity compiler settings to exclude metadata from the output.
    ///
    /// 3. Update Output Selection:
    ///    - It updates the file output selection settings in the Solidity compiler configuration
    ///      with the configured values.
    ///
    /// 4. Generate Standard JSON Input:
    ///    - It generates the standard JSON input for the contract using the `standard_json_input`
    ///      method of the project.
    ///    - The standard JSON input includes the contract's source code, compiler options, and file
    ///      output selection.
    ///
    /// 5. Build Artifacts Path:
    ///    - It builds the path for saving the compiler artifacts based on the contract source file.
    ///    - The artifacts will be saved in a directory named after the contract's filename within
    ///      the project's artifacts directory.
    ///
    /// 6. Save JSON Input:
    ///    - It saves the standard JSON input as a file named "json_input.json" within the
    ///      contract's artifacts directory.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let contract_path = PathBuf::from("/path/to/contract.sol");
    /// self.prepare_compiler_input(contract_path)?;
    /// ```
    ///
    /// In this example, the `prepare_compiler_input` function is called with the contract source
    /// path. It generates the JSON input for the contract, configures the Solidity compiler,
    /// and saves the input to the artifacts directory.
    fn prepare_compiler_input(
        &mut self,
        contract_path: &PathBuf,
        cache_base_path: &Path,
    ) -> Result<()> {
        let artifact_path = cache_base_path.to_path_buf();

        // Step 1: Configure File Output Selection
        let mut file_output_selection: FileOutputSelection = BTreeMap::default();
        file_output_selection
            .insert("*".to_string(), vec!["abi".to_string(), "evm.methodIdentifiers".to_string()]);
        file_output_selection.insert("".to_string(), vec!["metadata".to_string()]);

        // Step 2: Configure Solidity Compiler
        // zksolc requires metadata to be 'None'
        self.project.solc_config.settings.metadata = None;

        // Step 3: Update Output Selection
        self.project
            .solc_config
            .settings
            .output_selection
            .0
            .insert("*".to_string(), file_output_selection.clone());

        // Step 4: Generate Standard JSON Input
        let standard_json = self
            .project
            .standard_json_input(contract_path)
            .wrap_err("Could not get standard json input")?;
        // Convert the standard JSON input to the zk-specific standard JSON format for further
        // processing
        let mut std_zk_json = self.convert_to_zk_standard_json(standard_json);

        // Patch the libraries to be relative to the project root
        // NOTE: This is a temporary fix until zksolc supports relative paths
        for (mut path, details) in
            std::mem::take(&mut std_zk_json.settings.libraries.libs).into_iter()
        {
            if let Ok(patched) = path.strip_prefix(&self.project.paths.root) {
                path = patched.to_owned();
            }

            std_zk_json.settings.libraries.libs.insert(path, details);
        }

        // Store the generated standard JSON input in the ZkSolc instance
        self.standard_json = Some(std_zk_json.to_owned());

        // Step 5: Save JSON Input
        let json_input_path = artifact_path.join("json_input.json");

        std::fs::write(
            json_input_path,
            serde_json::to_string_pretty(&std_zk_json)
                .wrap_err("Could not serialize JSON input")?,
        )
        .wrap_err("Could not write JSON input file")?;

        Ok(())
    }

    fn convert_to_zk_standard_json(
        &self,
        input: StandardJsonCompilerInput,
    ) -> ZkStandardJsonCompilerInput {
        ZkStandardJsonCompilerInput {
            language: input.language,
            sources: input.sources,
            settings: Settings {
                remappings: input.settings.remappings,
                optimizer: self.config.settings.optimizer.clone(),
                metadata: input.settings.metadata,
                output_selection: input.settings.output_selection,
                libraries: input.settings.libraries,
                is_system: self.config.settings.is_system,
                force_evmla: self.config.settings.force_evmla,
                missing_libraries_path: self.config.settings.missing_libraries_path.clone(),
                are_libraries_missing: self.config.settings.are_libraries_missing,
                contracts_to_compile: self.config.settings.contracts_to_compile.clone(),
            },
        }
    }

    /// Retrieves the versioned sources for the Solidity contracts in the project. The versioned
    /// sources represent the contracts grouped by their corresponding Solidity compiler
    /// versions. The function performs the following steps to obtain the versioned sources:
    ///
    /// # Workflow:
    /// 1. Retrieve Project Sources:
    ///    - The function calls the `sources` method of the `Project` instance to obtain the
    ///      Solidity contract sources for the project.
    ///    - If the retrieval of project sources fails, an error is returned.
    ///
    /// 2. Resolve Graph of Sources and Versions:
    ///    - The function creates a graph using the `Graph::resolve_sources` method, passing the
    ///      project paths and the retrieved contract sources.
    ///    - The graph represents the relationships between the contract sources and their
    ///      corresponding Solidity compiler versions.
    ///    - If the resolution of the graph fails, an error is returned.
    ///
    /// 3. Extract Versions and Edges:
    ///    - The function extracts the versions and edges from the resolved graph.
    ///    - The `versions` variable contains a mapping of Solidity compiler versions to the
    ///      contracts associated with each version.
    ///    - The `edges` variable represents the edges between the contract sources and their
    ///      corresponding Solidity compiler versions.
    ///    - If the extraction of versions and edges fails, an error is returned.
    ///
    /// 4. Retrieve Solc Version:
    ///    - The function attempts to retrieve the Solidity compiler version associated with the
    ///      project.
    ///    - If the retrieval of the solc version fails, an error is returned.
    ///
    /// 5. Return Versioned Sources:
    ///    - The function returns a `BTreeMap` containing the versioned sources, where each entry in
    ///      the map represents a Solidity compiler version and its associated contracts.
    ///    - The map is constructed using the `solc_version` and `versions` variables.
    ///    - If the construction of the versioned sources map fails, an error is returned.
    ///
    /// # Arguments
    ///
    /// * `self` - A mutable reference to the `ZkSolc` instance.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `BTreeMap` of the versioned sources on success, or an
    /// `anyhow::Error` on failure.
    ///
    /// # Errors
    ///
    /// This function can return an error if any of the following occurs:
    /// - The retrieval of project sources fails.
    /// - The resolution of the graph of sources and versions fails.
    /// - The extraction of versions and edges from the resolved graph fails.
    /// - The retrieval of the Solidity compiler version associated with the project fails.
    /// - The construction of the versioned sources map fails.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use foundry_cli::cmd::forge::zksolc::ZkSolc;
    /// let mut zk_solc = ZkSolc::new(...);
    /// let versioned_sources = zk_solc.get_versioned_sources()?;
    /// ```
    ///
    /// In this example, a `ZkSolc` instance is created, and the `get_versioned_sources` method is
    /// called to retrieve the versioned sources for the Solidity contracts in the project.
    /// The resulting `BTreeMap` of versioned sources is stored in the `versioned_sources` variable.
    ///
    /// # Note
    ///
    /// The `get_versioned_sources` function is typically called internally within the `ZkSolc`
    /// struct to obtain the necessary versioned sources for contract compilation.
    /// The versioned sources can then be used for further processing or analysis.
    fn get_versioned_sources(&mut self) -> Result<BTreeMap<Solc, SolidityVersionSources>> {
        // Step 1: Retrieve Project Sources
        let sources = self.project.paths.read_input_files()?;

        // Step 2: Resolve Graph of Sources and Versions
        let graph = Graph::resolve_sources(&self.project.paths, sources)
            .wrap_err("Could not resolve sources")?;

        // Step 3: Extract Versions and Edges
        let (versions, _edges) = graph
            .into_sources_by_version(self.project.offline)
            .wrap_err("Could not match solc versions to files")?;

        // Step 4: Retrieve Solc Version
        versions.get(&self.project).wrap_err("Could not get solc")
    }

    /// Returns the appropriate file name for the conflicting file.
    ///
    /// This should ensure that the resulting `PathBuf` is conflict free, which could be possible if
    /// there are two separate contract files (in different folders) that contain the same contract:
    ///
    /// `src/A.sol::A`
    /// `src/nested/A.sol::A`
    ///
    /// Which would result in the same `PathBuf` if only the file and contract name is taken into
    /// account, [`Self::output_file`].
    ///
    /// This return a unique output file
    fn conflict_free_output_file(
        already_taken: &HashSet<PathBuf>,
        conflict: PathBuf,
        contract_file: impl AsRef<Path>,
        artifacts_folder: impl AsRef<Path>,
    ) -> PathBuf {
        let artifacts_folder = artifacts_folder.as_ref();
        let mut rel_candidate = conflict;
        if let Ok(stripped) = rel_candidate.strip_prefix(artifacts_folder) {
            rel_candidate = stripped.to_path_buf();
        }
        #[allow(clippy::redundant_clone)] // false positive
        let mut candidate = rel_candidate.clone();
        let contract_file = contract_file.as_ref();
        let mut current_parent = contract_file.parent();

        while let Some(parent_name) = current_parent.and_then(|f| f.file_name()) {
            // this is problematic if both files are absolute
            candidate = Path::new(parent_name).join(&candidate);
            let out_path = artifacts_folder.join(&candidate);
            if !already_taken.contains(&out_path) {
                trace!("found alternative output file={:?} for {:?}", out_path, contract_file);
                return out_path;
            }
            current_parent = current_parent.and_then(|f| f.parent());
        }

        // this means we haven't found an alternative yet, which shouldn't actually happen since
        // `contract_file` are unique, but just to be safe, handle this case in which case
        // we simply numerate the parent folder

        trace!("no conflict free output file found after traversing the file");

        let mut num = 1;

        loop {
            // this will attempt to find an alternate path by numerating the first component in the
            // path: `<root>+_<num>/....sol`
            let mut components = rel_candidate.components();
            let first = components.next().expect("path not empty");
            let name = first.as_os_str();
            let mut numerated = std::ffi::OsString::with_capacity(name.len() + 2);
            numerated.push(name);
            numerated.push("_");
            numerated.push(num.to_string());

            let candidate: PathBuf = Some(numerated.as_os_str())
                .into_iter()
                .chain(components.map(|c| c.as_os_str()))
                .collect();
            if !already_taken.contains(&candidate) {
                trace!("found alternative output file={:?} for {:?}", candidate, contract_file);
                return candidate;
            }

            num += 1;
        }
    }

    /// Checks if the contract has been ignored by the user in the configuration file.
    fn is_contract_ignored_in_config(&self, relative_path: &Path) -> bool {
        let filename = relative_path
            .file_name()
            .expect("Failed to get Contract filename.")
            .to_str()
            .expect("Invalid Contract filename.");
        let mut should_compile = match self.config.contracts_to_compile {
            Some(ref contracts_to_compile) => {
                //compare if there is some member of the vector contracts_to_compile
                // present in the filename
                contracts_to_compile.iter().any(|c| c.is_match(filename))
            }
            None => true,
        };

        should_compile = match self.config.avoid_contracts {
            Some(ref avoid_contracts) => {
                //compare if there is some member of the vector avoid_contracts
                // present in the filename
                !avoid_contracts.iter().any(|c| c.is_match(filename))
            }
            None => should_compile,
        };

        !should_compile
    }

    /// Returns the missing libraries from the zksolc output.
    fn get_missing_libraries_from_output(output: &[u8]) -> Result<Vec<ZkMissingLibrary>> {
        let output: ZkSolcCompilerOutput = serde_json::from_slice(output).unwrap();

        let mut missing_libraries = HashSet::new();

        // First get all the missing libraries
        output.contracts.iter().for_each(|(_path, inner_contracts)| {
            inner_contracts.iter().for_each(|(_contract_name, contract)| {
                if let Some(missing_libs) = &contract.missing_libraries {
                    missing_libs.iter().for_each(|lib| {
                        missing_libraries.insert(lib.clone());
                    });
                }
            })
        });

        let mut missing_library_dependencies: Vec<ZkMissingLibrary> = Vec::new();

        // Now get the missing libraries of each missing library
        for library in missing_libraries {
            let mut split = library.split(':');
            let lib_file_path = split.next().unwrap();
            let lib_contract_name = split.next().unwrap();

            output
                .contracts
                .get(lib_file_path)
                .and_then(|contract_map| contract_map.get(lib_contract_name))
                .iter()
                .for_each(|lib| {
                    missing_library_dependencies.push(ZkMissingLibrary {
                        contract_name: lib_contract_name.to_string(),
                        contract_path: lib_file_path.to_string(),
                        missing_libraries: lib.missing_libraries.clone().unwrap_or_default(),
                    });
                });
        }

        Ok(missing_library_dependencies)
    }

    fn run_compiler(
        &self,
        contract_path: &Path,
        solc: &Solc,
    ) -> Result<Option<std::process::Output>> {
        let get_compiler = |args| {
            let mut command = Command::new(&self.config.compiler_path);
            command.args(&args).stdin(Stdio::piped()).stderr(Stdio::piped()).stdout(Stdio::piped());
            command
        };

        let mut command = get_compiler(self.build_compiler_args(contract_path, solc, false));
        trace!(compiler_path = ?command.get_program(), args = ?command.get_args(), "Running compiler");

        let exec_compiler = |command: &mut Command| -> Result<std::process::Output> {
            let mut child = command.spawn().wrap_err("Failed to start the compiler")?;
            let stdin = child.stdin.take().expect("Stdin exists.");
            serde_json::to_writer(stdin, self.standard_json.as_ref().unwrap())
                .wrap_err("Could not assign standard_json to writer")?;
            child.wait_with_output().wrap_err("Could not run compiler cmd")
        };
        let mut output = exec_compiler(&mut command)?;

        // retry but detect missing libraries this time
        if !output.status.success() && Self::maybe_missing_libraries(&output.stderr) {
            command = get_compiler(self.build_compiler_args(contract_path, solc, true));
            trace!("Running compiler with missing libraries detection");
            output = exec_compiler(&mut command)?;
        }

        // Skip this file if the compiler output is empty
        // currently zksolc returns false for success if output is empty
        // when output is empty, it has a length of 3, `[]\n`
        // solc returns true for success if output is empty
        if !output.status.success() && output.stderr.len() <= 3 {
            return Ok(None)
        }

        if !output.status.success() {
            eyre::bail!(
                "Compilation failed with stdout: {:?} and stderr: {:?}. Using compiler: {:?}, with args {:?} {:?}",
                String::from_utf8(output.stdout).unwrap_or_default(),
                String::from_utf8(output.stderr).unwrap_or_default(),
                self.config.compiler_path,
                contract_path,
                command.get_args()
            );
        }

        Ok(Some(output))
    }

    fn maybe_missing_libraries(stderr: &[u8]) -> bool {
        stderr.windows(MISSING_LIBS_ERROR.len()).any(|window| window == MISSING_LIBS_ERROR)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ZkSolcCompilerOutput {
    // Map from file name -> (Contract name -> Contract)
    pub contracts: HashMap<String, HashMap<String, ZkContract>>,
    pub sources: HashMap<String, ZkSourceFile>,
    pub version: String,
    pub long_version: String,
    pub zk_version: String,
    pub errors: Vec<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZkContract {
    pub hash: Option<String>,
    // Hashmap from hash to filename:contract_name string.
    #[serde(rename = "factoryDependencies", default)]
    pub factory_dependencies: Option<HashMap<String, String>>,
    pub evm: Option<Evm>,
    pub abi: Option<JsonAbi>,
    pub missing_libraries: Option<Vec<String>>,
}
#[derive(Debug, Serialize, Deserialize)]

pub struct Evm {
    pub bytecode: Option<ZkSolcBytecode>,
}
#[derive(Debug, Serialize, Deserialize)]

pub struct ZkSolcBytecode {
    object: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ZkSourceFile {
    pub id: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ZkMissingLibrary {
    pub contract_name: String,
    pub contract_path: String,
    pub missing_libraries: Vec<String>,
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{ZkSolc, ZkSolcCompilerOutput};

    /// Basic test to analyze the single Counter.sol artifact.
    #[test]
    pub fn test_artifacts_extraction() {
        let data = include_str!("../../../../../testdata/artifacts-counter/artifacts.json")
            .as_bytes()
            .to_vec();
        let parsed_data: ZkSolcCompilerOutput = serde_json::from_slice(&data).unwrap();
        let mut displayed_warnings = HashSet::new();
        let source = "src/Counter.sol".to_owned();
        let result = ZkSolc::handle_output(&parsed_data, &source, &mut displayed_warnings);

        let artifact = result.get("src/Counter.sol/Counter.json").unwrap();
        assert_eq!(artifact.file.to_str(), Some("src/Counter.sol/Counter.json"));
        assert_eq!(artifact.version.to_string(), "0.8.20");
        assert!(artifact.artifact.abi.is_some());
        assert_eq!(artifact.artifact.bytecode.as_ref().unwrap().object.bytes_len(), 3883);
    }
    #[test]
    pub fn test_json_parsing() {
        let data = include_str!("../../../../../testdata/artifacts-counter/artifacts.json")
            .as_bytes()
            .to_vec();
        let _parsed: ZkSolcCompilerOutput = serde_json::from_slice(&data).unwrap();

        // Contract that has almost no data (and many fields missing).
        let almost_empty_data =
            include_str!("../../../../../testdata/artifacts-counter/empty.json")
                .as_bytes()
                .to_vec();
        let _parsed_empty: ZkSolcCompilerOutput =
            serde_json::from_slice(&almost_empty_data).unwrap();
    }
}
