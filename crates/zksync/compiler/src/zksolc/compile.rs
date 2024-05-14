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
use eyre::{Context, ContextCompat, Result};
use foundry_compilers::{
    artifacts::{
        output_selection::FileOutputSelection, CompactBytecode, CompactDeployedBytecode, Source,
        Sources, StandardJsonCompilerInput,
    },
    ArtifactFile, Artifacts, CompilerInput, ConfigurableContractArtifact, Graph, Project,
    ProjectCompileOutput, Solc,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{exit, Command, Stdio},
    str::FromStr,
};
use tracing::{error, info, trace, warn};
use zksync_basic_types::H256;

type ArtifactsMap<T> = <Artifacts<T> as core::ops::Deref>::Target;
type VersionedSources = BTreeMap<Solc, SolidityVersionSources>;

use crate::zksolc::PackedEraBytecode;

/// It is observed that when there's a missing library without
/// `--detect-missing-libraries` an error is thrown that contains
/// this message fragment
const MISSING_LIBS_ERROR: &[u8] = b"not found in the project".as_slice();

/// Mapping of bytecode hash (without "0x" prefix) to the respective contract name.
pub type ContractBytecodes = BTreeMap<String, String>;

#[derive(Debug, Default, Clone)]
pub struct ZkSolcArtifactPaths {
    base: PathBuf,
}

impl ZkSolcArtifactPaths {
    pub fn new(filename: PathBuf) -> Self {
        Self { base: filename }
    }

    /// Returns the path where to store the compiler output for this artifact
    pub fn artifact(&self) -> PathBuf {
        self.base.join("artifacts.json")
    }

    /// Returns the path where to store the source hash for this artifact
    pub fn hash(&self) -> PathBuf {
        self.base.join("contract_hash")
    }

    /// Returns the path where to store the compiler input for this artifact
    pub fn input(&self) -> PathBuf {
        self.base.join("json_input.json")
    }

    /// Ensures that the directory for these artifacts exists
    pub fn create(&self) -> Result<()> {
        fs::create_dir_all(&self.base).wrap_err("Failed creating artifacts dir")
    }
}

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
) -> eyre::Result<(ProjectCompileOutput, ContractBytecodes)> {
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
    pub fn compile(&mut self) -> Result<(ProjectCompileOutput, ContractBytecodes)> {
        let mut displayed_warnings = HashSet::new();
        let mut data = ArtifactsMap::new();
        // Step 1: Collect Source Files
        let (cached, sources) = self.get_versioned_sources().wrap_err("Cannot get source files")?;
        let mut contract_bytecodes = BTreeMap::new();

        let mut all_missing_libraries: HashSet<ZkMissingLibrary> = HashSet::new();
        let artifacts_path = self.project.paths.artifacts.clone();

        // Step 2: populate from cache
        cached.into_iter().for_each(|entry| {
            let output = Self::parse_compiler_output(entry);
            Self::handle_output(
                output,
                &mut displayed_warnings,
                &artifacts_path,
                None,
                &mut data,
                &mut contract_bytecodes,
            );
        });

        // Step 3: Proceed with contract compilation
        for (solc, (_, sources)) in sources {
            let total = sources.len();
            info!(solc = ?solc.solc, "\nCompiling {total} files...");
            let mut sp = spinoff::Spinner::new(
                spinoff::spinners::Dots8bit,
                format!("Compiling {total} files..."),
                None,
            );

            let input = self
                .prepare_compiler_input(sources)
                .wrap_err("Failed to prepare compiler inputs")?;

            // TODO: split compilation between is-system and non-system
            let Some(output) = self.run_compiler(&input, &solc)? else { continue };
            tracing::debug!(status = ?output.status, output = ?&output.stdout[..2048], "compiler output");

            let output = Self::parse_compiler_output(output.stdout);

            // TODO: recompile without contracts with missing libraries?
            all_missing_libraries.extend(Self::missing_libraries_iter(&output).1);

            // Step 6: Handle Output (Errors and Warnings)
            Self::handle_output(
                output,
                &mut displayed_warnings,
                &artifacts_path,
                Some(&input),
                &mut data,
                &mut contract_bytecodes,
            );

            sp.success(&format!("Compiled {total} files!"));
        }

        // Step 4: If missing library dependencies, save them to a file and return an error
        if !all_missing_libraries.is_empty() {
            let dependencies: Vec<ZkMissingLibrary> = all_missing_libraries.into_iter().collect();
            libraries::add_dependencies_to_missing_libraries_cache(
                &self.project.paths.root,
                dependencies.as_slice(),
            )?;
            eyre::bail!("Missing libraries detected {:?}\n\nRun the following command in order to deploy the missing libraries:\nforge create --deploy-missing-libraries --private-key <PRIVATE_KEY> --rpc-url <RPC_URL> --chain <CHAIN_ID> --zksync", dependencies);
        }

        let mut result = ProjectCompileOutput::default();
        result.set_compiled_artifacts(Artifacts(data));
        Ok((result, contract_bytecodes))
    }

    /// Checks if the contract has already been compiled for the given input contract hash.
    /// If yes, returns the pre-compiled data.
    fn check_cache(artifact_paths: &ZkSolcArtifactPaths, contract_hash: &str) -> Option<Vec<u8>> {
        let hash = artifact_paths.hash();
        let artifact = artifact_paths.artifact();

        if hash.exists() && artifact.exists() {
            File::open(&hash)
                .and_then(|mut file| {
                    let mut cached_contract_hash = String::new();
                    file.read_to_string(&mut cached_contract_hash).map(|_| cached_contract_hash)
                })
                .and_then(|cached_contract_hash| {
                    tracing::trace!(?artifact, ?hash, expected = ?contract_hash, cached = ?cached_contract_hash, "check_cache");
                    if cached_contract_hash == contract_hash {
                        Ok(Some(contract_hash))
                    } else {
                        Err(std::io::Error::new(std::io::ErrorKind::Other, "hashes do not match"))
                    }
                })
                .and_then(|_| {
                    File::open(artifact).and_then(|mut file| {
                        let mut buffer = Vec::new();
                        file.read_to_end(&mut buffer).map(|_| Some(buffer))
                    })
                })
                .ok()
                .flatten()
        } else {
            None
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
        solc: &'s Solc,
        detect_missing_libraries: bool,
    ) -> Vec<&'s str> {
        // Get the solc compiler path as a string
        let solc_path = solc.solc.to_str().expect("Given solc compiler path wasn't valid.");

        // Build compiler arguments
        let mut comp_args = vec!["--standard-json", "--solc", solc_path];

        // // Check if system mode is enabled or if the source path contains "is-system"
        // if self.config.settings.is_system ||
        //     contract_path
        //         .to_str()
        //         .expect("Given contract path wasn't valid.")
        //         .contains("is-system")
        // {
        //     comp_args.push("--system-mode");
        // }

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

    fn parse_compiler_output(compiler_output: Vec<u8>) -> ZkSolcCompilerOutput {
        match serde_json::from_slice(&compiler_output) {
            Ok(output) => output,
            Err(_) => {
                let parsed_json = serde_json::from_slice::<serde_json::Value>(&compiler_output);

                match parsed_json {
                    Ok(json) if json.get("errors").is_some() => {
                        let errors = json["errors"]
                            .as_array()
                            .expect("Expected 'errors' to be an array")
                            .iter()
                            .map(|e| {
                                serde_json::from_value(e.clone()).expect("Error parsing error")
                            })
                            .collect::<Vec<CompilerError>>();
                        // Handle errors in the output
                        ZkSolc::handle_output_errors(errors);
                    }
                    _ => info!("Failed to parse compiler output!"),
                }
                exit(1);
            }
        }
    }

    /// Handles the output of the Solidity compiler after the compilation process is completed. It
    /// processes the compiler output, handles errors and warnings, and saves the compiler
    /// artifacts.
    ///
    /// # Arguments
    ///
    /// * `output` - The output of the Solidity compiler as a `std::process::Output` struct.
    /// * `source` - The path of the contract source file that was compiled.
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
    /// let source = "/path/to/contract.sol".to_string();
    /// let mut displayed_warnings = HashSet::new();
    /// ZkSolc::handle_output(output, source, &mut displayed_warnings);
    /// ```
    ///
    /// In this example, the `handle_output` function is called with the compiler output, contract
    /// source, and a mutable set for displayed warnings. It processes the output, handles
    /// errors and warnings, and saves the artifacts.
    pub fn handle_output(
        compiler_output: ZkSolcCompilerOutput,
        displayed_warnings: &mut HashSet<String>,
        artifact_paths: impl AsRef<Path>,
        input: Option<&str>,
        result: &mut ArtifactsMap<ConfigurableContractArtifact>,
        contract_bytecodes: &mut BTreeMap<String, String>,
    ) {
        let artifact_paths = artifact_paths.as_ref();

        // Handle warnings in the output
        Self::handle_output_warnings(&compiler_output, displayed_warnings);

        let mut sources = compiler_output.sources;
        // Process each contract in the output
        for (source_path, contracts) in compiler_output.contracts.into_iter() {
            let source_hash = Self::hash_contract(source_path.as_ref())
                .wrap_err(format!("Unable to obtain contract hash for {source_path:?}"))
                .unwrap();

            let path = PathBuf::from(source_path.clone());
            let filename = path
                .file_name()
                .wrap_err(format!("Could not get filename from {:?}", source_path))
                .unwrap()
                .to_str()
                .expect("Invalid Contract filename");

            for (name, contract) in &contracts {
                // if contract hash is empty, skip
                if contract.hash.is_none() {
                    trace!("{name} -> empty contract.hash");
                    continue
                }
                if contract_bytecodes.get(contract.hash.as_ref().unwrap()).is_some() {
                    // if contract hash is already known, skip
                    trace!("{name} -> already known");
                    continue
                }

                contract_bytecodes.insert(contract.hash.clone().unwrap(), name.clone());

                // map factory dependencies by hash to bytecode
                let factory_deps = contract
                    .factory_dependencies
                    .as_ref()
                    .into_iter()
                    .flatten()
                    .map(|(hash, _)| hash)
                    // add current contract hash to list of factory deps
                    .chain(contract.hash.as_ref())
                    .map(|hash| H256::from_str(hash).expect("invalid factory dep bytecode hash"))
                    .collect();

                let packed_bytecode = Bytes::from(
                    PackedEraBytecode::new(
                        contract.hash.as_ref().unwrap(),
                        contract.evm.as_ref().unwrap().bytecode.as_ref().unwrap().object.as_str(),
                        factory_deps,
                    )
                    .to_vec(),
                );

                // TODO: add missing libs to `link_references`?
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

                let artifact = ArtifactFile {
                    artifact: art,
                    file: filename.into(),
                    version: Version::parse(&compiler_output.version).unwrap(),
                };
                // each contract is only supposed to have 1 artifact
                *result.entry(filename.to_string()).or_default().entry(name.clone()).or_default() =
                    vec![artifact];
            }

            let artifact = ZkSolcCompilerOutput {
                sources: sources.remove_entry(&source_path).into_iter().collect(),
                contracts: [(source_path, contracts)].into_iter().collect(),
                version: compiler_output.version.clone(),
                long_version: compiler_output.long_version.clone(),
                zk_version: compiler_output.zk_version.clone(),
                errors: Default::default(),
            };
            write_artifact(input, artifact, artifact_paths, filename, &source_hash);
        }

        /// Writes artifact to disk
        ///
        /// Args:
        /// * raw_compiler_input: stored in `input.json` - represents the standard json input to the
        ///   compiler
        /// * artifact_output: stored in `artifact.json` - represents the json output of the
        ///   compiler for this specific artifact
        /// * artifacts_paths: artifact output folder
        /// * filename: source contract filename
        /// * source_hash: hash of the contents of the source file
        /// * bytecode_hash: stored in `contract_hash` - contract bytecode hash
        /// * contract_name: name of the contract with the given bytecode hash
        fn write_artifact(
            raw_compiler_input: Option<&str>,
            artifact_output: ZkSolcCompilerOutput,
            artifacts_paths: impl AsRef<Path>,
            filename: &str,
            source_hash: &str,
        ) {
            let artifacts = artifacts_paths.as_ref().join(filename);

            artifact_output
                .contracts
                .values()
                .flat_map(|ccs| ccs.iter())
                .flat_map(|(name, c)| c.hash.as_ref().map(|h| (name, h)))
                .for_each(|(contract_name, bytecode_hash)| {
                    info!(?filename, "{contract_name} -> Bytecode Hash: {bytecode_hash}")
                });

            let artifacts = ZkSolcArtifactPaths::new(artifacts);
            artifacts.create().unwrap();

            if let Some(input) = raw_compiler_input {
                let mut json_input_file = File::create(artifacts.input())
                    .wrap_err("Could not create json_input file")
                    .unwrap();

                json_input_file
                    .write_all(input.as_bytes())
                    .unwrap_or_else(|e| panic!("Could not write input file: {}", e));
            }

            let artifacts_file = File::create(artifacts.artifact())
                .wrap_err("Could not create artifacts file")
                .unwrap();
            serde_json::to_writer(artifacts_file, &artifact_output)
                .unwrap_or_else(|e| panic!("Could not write artifacts file: {}", e));

            // Create the contract_hash file for saving the input contract hash
            let mut contract_hash_file = File::create(artifacts.hash())
                .wrap_err("Could not create contract_hash file")
                .unwrap();

            // Write the contract's file hash to the contract_hash file
            contract_hash_file
                .write_all(source_hash.as_bytes())
                .unwrap_or_else(|e| panic!("Could not write contract_hash file: {}", e));
        }
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
    /// # Example
    ///
    /// ```ignore
    /// let contract_path = PathBuf::from("/path/to/contract.sol");
    /// let json_input = self.prepare_compiler_input(contract_path)?;
    /// ```
    ///
    /// In this example, the `prepare_compiler_input` function is called with the contract source
    /// path. It generates the JSON input for the contract, configures the Solidity compiler,
    /// and returns the input to pass to the compiler.
    fn prepare_compiler_input(&mut self, sources: Sources) -> Result<String> {
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

        // from foundry_compilers
        fn rebase_path(base: impl AsRef<Path>, path: impl AsRef<Path>) -> PathBuf {
            // use path_slash::PathExt;

            let mut base_components = base.as_ref().components();
            let mut path_components = path.as_ref().components();

            let mut new_path = PathBuf::new();

            while let Some(path_component) = path_components.next() {
                let base_component = base_components.next();

                if Some(path_component) != base_component {
                    if base_component.is_some() {
                        new_path.extend(
                            std::iter::repeat(std::path::Component::ParentDir)
                                .take(base_components.count() + 1),
                        );
                    }

                    new_path.push(path_component);
                    new_path.extend(path_components);

                    break;
                }
            }

            //TODO: path_slash::PathExt::to_slash_lossy
            new_path.to_string_lossy().into_owned().into()
        }
        let root = self.project.root();
        let sources = sources
            .into_iter()
            .map(|(path, source)| (rebase_path(root, path), source.clone()))
            .collect();

        let mut compiler_input = CompilerInput::with_sources(sources);
        //FIXME: keep and process Yul sources?
        compiler_input.retain(|i| i.language == "Solidity");
        let mut compiler_input = compiler_input.pop().expect("No solidity compiler input");

        compiler_input.settings = self.project.solc_config.settings.clone();
        compiler_input.settings.remappings = self
            .project
            .paths
            .remappings
            .clone()
            .into_iter()
            .map(|r| r.into_relative(self.project.root()).to_relative_remapping())
            .collect::<Vec<_>>();

        // Step 4: Generate Standard JSON Input
        let standard_json = {
            let CompilerInput { language, sources, settings } = compiler_input;
            StandardJsonCompilerInput { language, sources: sources.into_iter().collect(), settings }
        };

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

        let serialized_input =
            serde_json::to_string(&std_zk_json).wrap_err("Could not serialize JSON input")?;

        // Store the generated standard JSON input in the ZkSolc instance
        self.standard_json = Some(std_zk_json);

        Ok(serialized_input)
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

    /// Retrieves the versioned sources for the Solidity contracts in the project and cached
    /// artifacts. The versioned sources represent the contracts grouped by their corresponding
    /// Solidity compiler versions. The function performs the following steps to obtain the
    /// versioned sources:
    ///
    /// # Workflow:
    /// 1. Retrieve Project Sources:
    ///    - The function calls the `sources` method of the `Project` instance to obtain the
    ///      Solidity contract sources for the project.
    ///    - If the retrieval of project sources fails, an error is returned.
    ///
    /// 2. Filter out cached Sources:
    ///    - The function filters out any sources that are ignored or cached.
    ///    - If the sources are cached the corresponding artifact is retrieved.
    ///
    /// 3. Resolve Graph of Sources and Versions:
    ///    - The function creates a graph using the `Graph::resolve_sources` method, passing the
    ///      project paths and the retrieved contract sources.
    ///    - The graph represents the relationships between the contract sources and their
    ///      corresponding Solidity compiler versions.
    ///    - If the resolution of the graph fails, an error is returned.
    ///
    /// 4. Extract Versions and Edges:
    ///    - The function extracts the versions and edges from the resolved graph.
    ///    - The `versions` variable contains a mapping of Solidity compiler versions to the
    ///      contracts associated with each version.
    ///    - The `edges` variable represents the edges between the contract sources and their
    ///      corresponding Solidity compiler versions.
    ///    - If the extraction of versions and edges fails, an error is returned.
    ///
    /// 5. Retrieve Solc Version:
    ///    - The function attempts to retrieve the Solidity compiler version associated with the
    ///      project.
    ///    - If the retrieval of the solc version fails, an error is returned.
    ///
    /// 6. Return Versioned Sources:
    ///    - The function returns a `BTreeMap` containing the versioned sources, where each entry in
    ///      the map represents a Solidity compiler version and its associated contracts.
    ///    - The map is constructed using the `solc_version` and `versions` variables.
    ///    - The function also returns a vector of bytes which represent the cached artifacts.
    ///    - If the construction of the versioned sources map fails, an error is returned.
    ///
    /// # Arguments
    ///
    /// * `self` - A reference to the `ZkSolc` instance.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `BTreeMap` of the versioned sources
    /// and a `Vec` of the artifacts present in cache on success, or an
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
    fn get_versioned_sources(&self) -> Result<(Vec<Vec<u8>>, VersionedSources)> {
        let artifacts_paths = &self.project.paths.artifacts;

        // Step 1: Retrieve Project Sources
        let mut sources = self.project.paths.read_input_files()?;
        let mut cache = Vec::with_capacity(sources.len());
        sources.retain(|path, _| {
            let relative_path = path.strip_prefix(self.project.root()).unwrap_or(path.as_ref());
            let is_ignored = self.is_contract_ignored_in_config(relative_path);

            //TODO: feed cached artifacts to compiler?
            //TODO: retrieve cached artifacts for dependencies too?
            // that would also mean we need to first resolve, then filter
            // and resolve again with new sources
            let cached =
                Self::check_contract_is_cached(artifacts_paths, path).ok().and_then(|r| r.0);

            // prune ignored or cached contractacs
            match (is_ignored, cached) {
                (false, None) => true,
                (true, _) => false,
                (false, Some(cached)) => {
                    cache.push(cached);
                    false
                }
            }
        });

        // Step 2: Resolve Graph of Sources and Versions
        let graph = Graph::resolve_sources(&self.project.paths, sources)
            .wrap_err("Could not resolve sources")?;

        // Step 3: Extract Versions and Edges
        let (versions, _edges) = graph
            .into_sources_by_version(self.project.offline)
            .wrap_err("Could not match solc versions to files")?;

        // Step 4: Retrieve Solc Version
        versions.get(&self.project).wrap_err("Could not get solc").map(|s| (cache, s))
    }

    /// Checks if the contract has been ignored by the user in the configuration file.
    fn is_contract_ignored_in_config(&self, relative_path: &Path) -> bool {
        let mut should_compile = match self.config.contracts_to_compile {
            Some(ref contracts_to_compile) => {
                //compare if there is some member of the vector contracts_to_compile
                // present in the filename
                contracts_to_compile.iter().any(|c| c.is_match(relative_path))
            }
            None => true,
        };

        should_compile = match self.config.avoid_contracts {
            Some(ref avoid_contracts) => {
                //compare if there is some member of the vector avoid_contracts
                // present in the filename
                !avoid_contracts.iter().any(|c| c.is_match(relative_path))
            }
            None => should_compile,
        };

        !should_compile
    }

    /// Checks if the contract has already been compiled for the given contract path.
    fn check_contract_is_cached(
        artifacts_path: impl AsRef<Path>,
        contract_path: impl AsRef<Path>,
    ) -> Result<(Option<Vec<u8>>, String)> {
        let contract_path = contract_path.as_ref();
        trace!(?contract_path, "checking cache");
        let contract_hash =
            Self::hash_contract(contract_path).wrap_err("Trying to hash contract contents")?;
        let artifact_paths = ZkSolcArtifactPaths::new(
            artifacts_path.as_ref().join(
                contract_path
                    .file_name()
                    .wrap_err(format!("Could not get filename from {:?}", contract_path))?,
            ),
        );
        let entry = Self::check_cache(&artifact_paths, &contract_hash);
        if entry.is_some() {
            trace!(?contract_path, "cache hit!");
        }

        Ok((entry, contract_hash))
    }

    /// Returns the hash of the contract at the given path.
    fn hash_contract(contract_path: &Path) -> Result<String> {
        let mut contract_file = File::open(contract_path)?;
        let mut buffer = Vec::new();
        contract_file.read_to_end(&mut buffer)?;
        let contract_hash = hex::encode(xxhash_rust::const_xxh3::xxh3_64(&buffer).to_be_bytes());
        Ok(contract_hash)
    }

    /// Returns the missing libraries from the zksolc output and what contracts contain missing
    /// libraries
    fn missing_libraries_iter(
        output: &'_ ZkSolcCompilerOutput,
    ) -> (impl Iterator<Item = &'_ ZkContract>, impl Iterator<Item = ZkMissingLibrary> + '_) {
        let contracts_with_libs = output
            .contracts
            .values()
            .flat_map(|ccs| ccs.iter())
            .filter(|(_, contract)| {
                !contract
                    .missing_libraries
                    .as_ref()
                    .map(|libs| !libs.is_empty())
                    .unwrap_or_default()
            })
            .map(|(_, c)| c);

        let libs = contracts_with_libs.clone()

            .flat_map(|contract| contract.missing_libraries.iter().flatten())
            .map(|library| {
let mut split = library.split(':');
            let path = split.next().expect("missing library format {{path}}:{{contract_name}}; unable to parse path ");
            let contract_name = split.next().expect("missing library format {{path}}:{{contract_name}}; unable to parse contract_name");
                (path, contract_name)
            }).filter_map(|(path, name)| {
output
                .contracts
                .get(path)
                .and_then(|contract_map| contract_map.get(name))
                .map(|lib| {
                    ZkMissingLibrary {
                        contract_name: name.to_string(),
                        contract_path: path.to_string(),
                        missing_libraries: lib.missing_libraries.clone().unwrap_or_default(),
                    }
                })
            });

        (contracts_with_libs, libs)
    }

    fn run_compiler(&self, json_input: &str, solc: &Solc) -> Result<Option<std::process::Output>> {
        let get_compiler = |args| {
            let mut command = Command::new(&self.config.compiler_path);
            command.args(&args).stdin(Stdio::piped()).stderr(Stdio::piped()).stdout(Stdio::piped());
            command
        };

        let mut command = get_compiler(self.build_compiler_args(solc, false));
        trace!(compiler_path = ?command.get_program(), args = ?command.get_args(), "Running compiler");

        let exec_compiler = |command: &mut Command| -> Result<std::process::Output> {
            let mut child = command.spawn().wrap_err("Failed to start the compiler")?;
            let mut stdin = child.stdin.take().expect("Stdin exists.");
            stdin
                .write_all(json_input.as_bytes())
                .wrap_err("Could not assign standard_json to writer")?;
            std::mem::drop(stdin);

            child.wait_with_output().wrap_err("Could not run compiler cmd")
        };
        let mut output = exec_compiler(&mut command)?;

        // retry but detect missing libraries this time
        if !output.status.success() && Self::maybe_missing_libraries(&output.stderr) {
            command = get_compiler(self.build_compiler_args(solc, true));
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
                "Compilation failed with stdout: {:?} and stderr: {:?}. Using compiler: {:?}, with args {:?}",
                String::from_utf8(output.stdout).unwrap_or_default(),
                String::from_utf8(output.stderr).unwrap_or_default(),
                self.config.compiler_path,
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
    #[serde(default)]
    pub contracts: HashMap<String, HashMap<String, ZkContract>>,
    #[serde(default)]
    pub sources: HashMap<String, ZkSourceFile>,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub long_version: String,
    #[serde(default)]
    pub zk_version: String,
    #[serde(default)]
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

#[derive(Debug, Clone, Deserialize, Serialize, Hash, PartialEq, Eq)]
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
        let mut displayed_warnings = HashSet::new();
        let mut result = Default::default();
        ZkSolc::handle_output(
            ZkSolc::parse_compiler_output(data),
            &mut displayed_warnings,
            "/tmp",
            None,
            &mut result,
            &mut Default::default(),
        );

        let artifacts = result.get("src/Counter.sol").unwrap().get("Counter").unwrap();
        assert_eq!(artifacts.len(), 1);
        let first = &artifacts[0];
        assert_eq!(first.file.to_str(), Some("Counter.sol"));
        assert_eq!(first.version.to_string(), "0.8.20");
        assert!(first.artifact.abi.is_some());
        assert_eq!(first.artifact.bytecode.as_ref().unwrap().object.bytes_len(), 3883);
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
