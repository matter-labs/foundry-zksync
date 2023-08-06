/// This module provides the implementation of the ZkSolc compiler for Solidity contracts.
/// ZkSolc is a specialized compiler that supports zero-knowledge (ZK) proofs for smart contracts.
///
/// The `ZkSolc` struct represents an instance of the compiler, and it is responsible for compiling
/// Solidity contracts and handling the output. It uses the `solc` library to interact with the Solidity compiler.
///
/// The `ZkSolc` struct provides the following functionality:
///
/// - Configuration: It allows configuring the compiler path, system mode, and force-evmla options through
///   the `ZkSolcOpts` struct.
///
/// - Compilation: The `compile` method initiates the compilation process. It collects the source files,
///   parses the JSON input, builds compiler arguments, runs the compiler, and handles the output.
///
/// - Error and Warning Handling: The compiler output is checked for errors and warnings, and they are
///   displayed appropriately. If errors are encountered, the process will exit with a non-zero status code.
///
/// - JSON Input Generation: The `parse_json_input` method generates the JSON input required by the compiler
///   for each contract. It configures the Solidity compiler, saves the input to the artifacts directory, and
///   handles the output.
///
/// - Source Management: The `get_versioned_sources` method retrieves the project sources, resolves the graph
///   of sources and versions, and returns the sources grouped by Solc version.
///
/// - Artifact Path Generation: The `build_artifacts_path` and `build_artifacts_file` methods construct the
///   path and file for saving the compiler output artifacts.
use ansi_term::Colour::{Red, Yellow};
use anyhow::{Error, Result};
use ethers::prelude::{artifacts::Source, remappings::RelativeRemapping, Solc};
use ethers::solc::{
    artifacts::{output_selection::FileOutputSelection, StandardJsonCompilerInput},
    Graph, Project,
};
use regex::Regex;
use semver::Version;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    fmt, fs,
    fs::File,
    io::Write,
    path::PathBuf,
    process::{exit, Command, Stdio},
};

#[derive(Debug, Clone)]
pub struct ZkSolcOpts {
    pub remappings: Vec<RelativeRemapping>,
    pub compiler_path: PathBuf,
    pub is_system: bool,
    pub force_evmla: bool,
}

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
/// - `remappings`: A vector of relative remappings for the contracts.
///
/// Functionality:
/// - `new`: Constructs a new `ZkSolc` instance using the provided compiler path, project
///   configurations, and options.
/// - `compile`: Responsible for compiling the contracts in the project's 'sources' directory
///   and its subdirectories.
///
/// Error Handling:
/// - The methods in this struct return the `Result` type from the `anyhow` crate for flexible
///   and easy-to-use error handling.
///
/// Example Usage:
/// ```rust
/// use zk_solc::{ZkSolc, Project};
///
/// // Set the compiler path and other options
/// let compiler_path = "/path/to/zksolc";
/// let project = Project::new(...);
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
    project: Project,
    compiler_path: PathBuf,
    is_system: bool,
    force_evmla: bool,
    standard_json: Option<StandardJsonCompilerInput>,
    sources: Option<BTreeMap<Solc, (Version, BTreeMap<PathBuf, Source>)>>,
    remappings: Vec<RelativeRemapping>,
}

impl fmt::Display for ZkSolc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ZkSolc (
                compiler_path: {},   
                output_path: {},
            )",
            self.compiler_path.display(),
            self.project.paths.artifacts.display(),
        )
    }
}

impl ZkSolc {
    pub fn new(opts: ZkSolcOpts, project: Project) -> Self {
        Self {
            project,
            compiler_path: opts.compiler_path,
            is_system: opts.is_system,
            force_evmla: opts.force_evmla,
            standard_json: None,
            sources: None,
            remappings: opts.remappings,
        }
    }

    /// Compiles the Solidity contracts using the ZkSolc compiler.
    ///
    /// This applies remappings, parses JSON input, runs the compiler,
    /// and handles output for each contract source file.
    ///
    /// # Workflow
    ///
    /// 1. Collect Sources
    ///    - Calls `configure_solc()` to get sources
    ///
    /// 2. Loop Through Sources
    ///    - Filter for sources in 'sources' directory
    ///
    /// 3. Configure Compiler
    ///    - Calls `configure_compiler_output_settings()`
    ///  
    /// 4. Apply Remappings
    ///    - Calls `remap_source_content()`
    ///
    /// 5. Parse JSON Input
    ///    - Calls `parse_json_input()`
    ///
    /// 6. Build Arguments
    ///    - Calls `build_compiler_args()`
    ///
    /// 7. Run Compiler
    ///    - Executes the compiler process
    ///
    /// 8. Handle Output (Errors and Warnings):
    ///    - It handles the output of the compiler, extracting errors and warnings.
    ///    - Errors are printed in red, and warnings are printed in yellow.///    - Calls `handle_output()`
    ///
    /// Returns Ok if successful.
    ///
    /// # Note  
    ///
    /// This function stores parsed JSON input and versioned sources
    /// on the `ZkSolc` instance for later access.
    pub fn compile(mut self) -> Result<()> {
        // Step 1: Configure compiler
        self.configure_solc();
        self.configure_compiler_output_settings();
        let mut displayed_warnings = HashSet::new();

        // Step 2: Collect Source Files
        let sources = self.sources.clone().unwrap();

        // Step 3: Loop Through Sources
        for (solc, version) in sources {
            //configure standard json input fr each source
            for (contract_path, _) in version.1.clone() {
                // Check if the contract_path is in 'sources' directory or its subdirectories
                let is_in_sources_dir = contract_path
                    .ancestors()
                    .any(|ancestor| ancestor.starts_with(&self.project.paths.sources));

                // Skip this file if it's not in the 'sources' directory or its subdirectories
                if !is_in_sources_dir {
                    continue;
                }

                // get standard_json for this contract
                let mut standard_json = self.project.standard_json_input(&contract_path).unwrap();

                // Step 4: Apply remappings for each contract dependency
                for _source in &mut standard_json.sources {
                    _source.1.content = self.remap_source_content(_source.1.content.clone());
                }

                self.standard_json = Some(standard_json);

                // Step 5: Parse JSON Input for each Source
                if let Err(err) = self.parse_json_input(&contract_path) {
                    eprintln!("Failed to parse json input for zksolc compiler: {}", err);
                }

                // Step 6: Build Compiler Arguments
                let comp_args = self.build_compiler_args(&contract_path, &solc);

                // Step 7: Run Compiler
                let mut cmd = Command::new(&self.compiler_path);
                let mut child = cmd
                    .args(&comp_args)
                    .stdin(Stdio::piped())
                    .stderr(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn();
                let stdin = child.as_mut().unwrap().stdin.take().expect("Stdin exists.");

                serde_json::to_writer(stdin, &self.standard_json.clone().unwrap()).map_err(
                    |e| Error::msg(format!("Could not assign standard_json to writer: {}", e)),
                )?;

                // Step 8: Handle Output (Errors and Warnings)
                let output = child
                    .unwrap()
                    .wait_with_output()
                    .map_err(|e| Error::msg(format!("Could not run compiler cmd: {}", e)))?;

                let filename = contract_path
                    .file_name()
                    .expect("Failed to extract filename")
                    .to_str()
                    .expect("Failed to convert filename to str");

                self.handle_output(output, filename.to_string(), &mut displayed_warnings);
            }
        }

        // Return Ok if the compilation process completes without errors
        Ok(())
    }

    /// Applies import remappings to the provided source content.
    ///
    /// This is called from `compile()` to remap each contract source.
    ///
    /// It replaces import paths with placeholders, then replaces the
    /// placeholders with relative paths from the remappings.
    ///
    /// This was necessary to prevent the same imports from getting
    /// rewritten by similar remappings being auto generated by Foundry.
    fn remap_source_content(&mut self, source_content: String) -> String {
        let content = source_content;

        // Get relative remappings
        let remappings = &self.remappings.clone();

        // Replace imports with placeholders
        let content = replace_imports_with_placeholders(content, &remappings);

        // Substitute remapped paths
        let content = substitute_remapped_paths(content, &remappings);
        // Return the remapped source content
        content
    }

    /// Builds the compiler arguments for the Solidity compiler based on the provided versioned source
    /// and solc instance. The compiler arguments specify options and settings for the compiler's execution.
    ///
    /// # Arguments
    ///
    /// * `versioned_source` - A tuple containing the contract source path (`PathBuf`) and the corresponding
    ///                         `Source` object.
    /// * `solc` - The `Solc` instance representing the specific version of the Solidity compiler.
    ///
    /// # Returns
    ///
    /// A vector of strings representing the compiler arguments.
    fn build_compiler_args(&mut self, contract_path: &PathBuf, solc: &Solc) -> Vec<String> {
        // Get the solc compiler path as a string
        let solc_path = solc
            .solc
            .to_str()
            .unwrap_or_else(|| panic!("Error configuring solc compiler."))
            .to_string();

        // Build compiler arguments
        let mut comp_args = Vec::<String>::new();
        comp_args.push("--standard-json".to_string());
        comp_args.push("--solc".to_string());
        comp_args.push(solc_path.to_owned());

        // Check if system mode is enabled or if the source path contains "is-system"
        if self.is_system || contract_path.to_str().unwrap().contains("is-system") {
            comp_args.push("--system-mode".to_string());
        }

        // Check if force-evmla is enabled
        if self.force_evmla {
            comp_args.push("--force-evmla".to_string());
        }
        comp_args
    }

    /// Handles the output of the Solidity compiler after the compilation process is completed. It processes
    /// the compiler output, handles errors and warnings, and saves the compiler artifacts.
    ///
    /// # Arguments
    ///
    /// * `output` - The output of the Solidity compiler as a `std::process::Output` struct.
    /// * `source` - The path of the contract source file that was compiled.
    /// * `displayed_warnings` - A mutable set that keeps track of displayed warnings to avoid duplicates.
    ///
    /// # Output Handling
    ///
    /// - The output of the Solidity compiler is expected to be in JSON format.
    /// - The output is deserialized into a `serde_json::Value` object for further processing.
    ///
    /// # Error and Warning Handling
    ///
    /// - The function checks for errors and warnings in the compiler output and handles them accordingly.
    /// - Errors are printed in red color.
    /// - Warnings are printed in yellow color.
    /// - If an error is encountered, the function exits with a non-zero status code.
    /// - If only warnings are present, a message indicating the presence of warnings is printed.
    ///
    /// # Artifacts Saving
    ///
    /// - The function saves the compiler output (artifacts) in a file.
    /// - The artifacts are saved in a file named "artifacts.json" within the contract's artifacts directory.
    ///
    /// # Example
    ///
    /// ```rust
    /// let output = std::process::Output { ... };
    /// let source = "/path/to/contract.sol".to_string();
    /// let mut displayed_warnings = HashSet::new();
    /// self.handle_output(output, source, &mut displayed_warnings);
    /// ```
    ///
    /// In this example, the `handle_output` function is called with the compiler output, contract source,
    /// and a mutable set for displayed warnings. It processes the output, handles errors and warnings, and
    /// saves the artifacts.
    fn handle_output(
        &self,
        output: std::process::Output,
        source: String,
        displayed_warnings: &mut HashSet<String>,
    ) {
        // Deserialize the compiler output into a serde_json::Value object
        let output_json: Value = serde_json::from_slice(&output.clone().stdout)
            .unwrap_or_else(|e| panic!("Could not parse zksolc compiler output: {}", e));

        // Handle errors and warnings in the output
        self.handle_output_errors(&output_json, displayed_warnings);

        // Create the artifacts file for saving the compiler output
        let mut artifacts_file = self
            .build_artifacts_file(source.clone())
            .unwrap_or_else(|e| panic!("Error configuring solc compiler: {}", e));

        // Get the bytecode hashes for each contract in the output
        let output_obj = output_json["contracts"].as_object().unwrap();
        for key in output_obj.keys() {
            if key.contains(&source) {
                let b_code = output_obj[key].clone();
                let b_code_obj = b_code.as_object().unwrap();
                let b_code_keys = b_code_obj.keys();
                for hash in b_code_keys {
                    if let Some(bcode_hash) = b_code_obj[hash]["hash"].as_str() {
                        println!("{}", format!("{} -> Bytecode Hash: {} ", hash, bcode_hash));
                    }
                }
            }
        }

        // Beautify the output JSON
        let output_json_pretty = serde_json::to_string_pretty(&output_json)
            .unwrap_or_else(|e| panic!("Could not beautify zksolc compiler output: {}", e));

        // Write the beautified output JSON to the artifacts file
        artifacts_file
            .write_all(output_json_pretty.as_bytes())
            .unwrap_or_else(|e| panic!("Could not write artifacts file: {}", e));
    }

    /// Handles the errors and warnings present in the output JSON from the compiler.
    ///
    /// # Arguments
    ///
    /// * `output_json` - A reference to the output JSON from the compiler, represented as a `Value` from
    ///   the `serde_json` crate.
    /// * `displayed_warnings` - A mutable reference to a `HashSet` that tracks displayed warnings to
    ///   avoid duplicates.
    ///
    /// # Behavior
    ///
    /// This function iterates over the `errors` array in the output JSON and processes each error or
    /// warning individually. For each error or warning, it extracts the severity and formatted message
    /// from the JSON. If the severity is "warning", it checks if the same warning message has been
    /// displayed before to avoid duplicates. If the warning message has not been displayed before, it
    /// adds the message to the `displayed_warnings` set, prints the formatted warning message in
    /// yellow, and sets the `has_warning` flag to true. If the severity is not "warning", it prints
    /// the formatted error message in red and sets the `has_error` flag to true.
    ///
    /// If any errors are encountered, the function calls `exit(1)` to terminate the program. If only
    /// warnings are encountered, it prints a message indicating that the compiler run completed with
    /// warnings.
    fn handle_output_errors(&self, output_json: &Value, displayed_warnings: &mut HashSet<String>) {
        let errors = output_json
            .get("errors")
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| panic!("Could not find 'errors' array in the output JSON"));

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
            println!("Compiler run completed with warnings");
        }
    }

    /// Parses the JSON input for the compiler from the contract source.
    ///
    /// # Workflow
    ///
    /// 1. Build Artifacts Path
    /// 2. Save JSON Input
    ///
    /// # Arguments
    ///  
    /// * `contract_path` - Path to the contract source
    fn parse_json_input(&mut self, contract_path: &PathBuf) -> Result<()> {
        //Build Artifacts Path
        let artifact_path = &self
            .build_artifacts_path(contract_path)
            .map_err(|e| Error::msg(format!("Could not build_artifacts_path: {}", e)))
            .unwrap();

        let standard_json = match &self.standard_json {
            Some(json) => json,
            None => return Err(Error::msg("Standard JSON is not set")),
        };

        // Save JSON Input
        let json_input_path = artifact_path.join("json_input.json");
        let stdjson = serde_json::to_value(standard_json)
            .map_err(|e| Error::msg(format!("Could not serialize standard JSON input: {}", e)))?;
        std::fs::write(&json_input_path, serde_json::to_string_pretty(&stdjson).unwrap())
            .map_err(|e| Error::msg(format!("Could not write JSON input file: {}", e)))?;

        Ok(())
    }

    fn configure_solc(&mut self) {
        self.sources = Some(self.get_versioned_sources().unwrap());
    }

    /// Retrieves the versioned sources for the Solidity contracts in the project. The versioned sources
    /// represent the contracts grouped by their corresponding Solidity compiler versions. The function
    /// performs the following steps to obtain the versioned sources:
    ///
    /// # Workflow:
    /// 1. Retrieve Project Sources:
    ///    - The function calls the `sources` method of the `Project` instance to obtain the Solidity
    ///      contract sources for the project.
    ///    - If the retrieval of project sources fails, an error is returned.
    ///
    /// 2. Resolve Graph of Sources and Versions:
    ///    - The function creates a graph using the `Graph::resolve_sources` method, passing the project
    ///      paths and the retrieved contract sources.
    ///    - The graph represents the relationships between the contract sources and their corresponding
    ///      Solidity compiler versions.
    ///    - If the resolution of the graph fails, an error is returned.
    ///
    /// 3. Extract Versions and Edges:
    ///    - The function extracts the versions and edges from the resolved graph.
    ///    - The `versions` variable contains a mapping of Solidity compiler versions to the contracts
    ///      associated with each version.
    ///    - The `edges` variable represents the edges between the contract sources and their corresponding
    ///      Solidity compiler versions.
    ///    - If the extraction of versions and edges fails, an error is returned.
    ///
    /// 4. Retrieve Solc Version:
    ///    - The function attempts to retrieve the Solidity compiler version associated with the project.
    ///    - If the retrieval of the solc version fails, an error is returned.
    ///
    /// 5. Return Versioned Sources:
    ///    - The function returns a `BTreeMap` containing the versioned sources, where each entry in the
    ///      map represents a Solidity compiler version and its associated contracts.
    ///    - The map is constructed using the `solc_version` and `versions` variables.
    ///    - If the construction of the versioned sources map fails, an error is returned.
    ///
    /// # Arguments
    ///
    /// * `self` - A mutable reference to the `ZkSolc` instance.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `BTreeMap` of the versioned sources on success, or an `anyhow::Error` on failure.
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
    /// ```rust
    /// let mut zk_solc = ZkSolc::new(...);
    /// let versioned_sources = zk_solc.get_versioned_sources()?;
    /// ```
    ///
    /// In this example, a `ZkSolc` instance is created, and the `get_versioned_sources` method is called
    /// to retrieve the versioned sources for the Solidity contracts in the project.
    /// The resulting `BTreeMap` of versioned sources is stored in the `versioned_sources` variable.
    ///
    /// # Note
    ///
    /// The `get_versioned_sources` function is typically called internally within the `ZkSolc` struct to
    /// obtain the necessary versioned sources for contract compilation.
    /// The versioned sources can then be used for further processing or analysis.
    fn get_versioned_sources(
        &mut self,
    ) -> Result<BTreeMap<Solc, (Version, BTreeMap<PathBuf, Source>)>> {
        // Step 1: Retrieve Project Sources
        let sources = self
            .project
            .sources()
            .map_err(|e| Error::msg(format!("Could not get project sources: {}", e)))?;

        // Step 2: Resolve Graph of Sources and Versions
        let graph = Graph::resolve_sources(&self.project.paths, sources)
            .map_err(|e| Error::msg(format!("Could not create graph: {}", e)))?;

        // Step 3: Extract Versions and Edges
        let (versions, _edges) = graph
            .into_sources_by_version(self.project.offline)
            .map_err(|e| Error::msg(format!("Could not get versions & edges: {}", e)))?;

        // Step 4: Retrieve Solc Version
        let solc_version = versions
            .get(&self.project)
            .map_err(|e| Error::msg(format!("Could not get solc: {}", e)));

        solc_version
    }

    /// Builds the path for saving the artifacts (compiler output) of a contract based on the contract's source file.
    /// The function performs the following steps to construct the artifacts path:
    ///
    /// # Workflow:
    /// 1. Extract Filename:
    ///    - The function extracts the filename from the provided contract source path using the `file_name` method.
    ///    - If the extraction of the filename fails, an error is returned.
    ///
    /// 2. Build Artifacts Path:
    ///    - The function constructs the artifacts path by joining the extracted filename with the project's artifacts directory path.
    ///    - The `join` method is used on the `artifacts` directory path, passing the extracted filename.
    ///
    /// 3. Create Artifacts Directory:
    ///    - The function creates the artifacts directory and all its parent directories using the `create_dir_all` method from the `fs` module.
    ///    - If the creation of the artifacts directory fails, an error is returned.
    ///
    /// # Arguments
    ///
    /// * `self` - A reference to the `ZkSolc` instance.
    /// * `source` - The contract source path represented as a `PathBuf`.
    ///
    /// # Returns
    ///
    /// A `Result` containing the constructed artifacts path (`PathBuf`) on success, or an `anyhow::Error` on failure.
    ///
    /// # Errors
    ///
    /// This function can return an error if any of the following occurs:
    /// - The extraction of the filename from the contract source path fails.
    /// - The creation of the artifacts directory fails.
    fn build_artifacts_path(&self, source: &PathBuf) -> Result<PathBuf, anyhow::Error> {
        let filename = source.file_name().expect("Failed to get Contract filename.");
        let path = self.project.paths.artifacts.join(filename);
        fs::create_dir_all(&path)
            .map_err(|e| Error::msg(format!("Could not create artifacts directory: {}", e)))?;
        Ok(path)
    }

    /// Builds the file path for the artifacts (compiler output) of a contract based on the contract's source file and the project's artifacts directory.
    /// The function performs the following steps to construct the artifacts file path:
    ///
    /// # Workflow:
    /// 1. Build Artifacts File Path:
    ///    - The function constructs the file path for the artifacts file by joining the project's artifacts directory path, the contract's source file path, and the "artifacts.json" filename.
    ///    - The `join` method is used on the artifacts directory path, passing the contract's source file path joined with the "artifacts.json" filename.
    ///
    /// 2. Create Artifacts File:
    ///    - The function creates the artifacts file at the constructed file path using the `create` method from the `File` struct.
    ///    - If the creation of the artifacts file fails, an error is returned.
    ///
    /// # Arguments
    ///
    /// * `self` - A reference to the `ZkSolc` instance.
    /// * `source` - The contract source file represented as a `String`.
    ///
    /// # Returns
    ///
    /// A `Result` containing the created `File` object on success, or an `anyhow::Error` on failure.
    ///
    /// # Errors
    ///
    /// This function can return an error if the creation of the artifacts file fails.
    fn build_artifacts_file(&self, source: String) -> Result<File> {
        File::create(self.project.paths.artifacts.join(source).join("artifacts.json"))
            .map_err(|e| Error::msg(format!("Could not create artifacts file: {}", e)))
    }

    fn configure_compiler_output_settings(&mut self) {
        // Configure File Output Selection
        let mut file_output_selection: FileOutputSelection = BTreeMap::default();
        file_output_selection.insert(
            "*".to_string(),
            vec![
                "abi".to_string(),
                "evm.methodIdentifiers".to_string(),
                // "evm.legacyAssembly".to_string(),
            ],
        );
        file_output_selection.insert(
            "".to_string(),
            vec![
                "metadata".to_string(),
                // "ast".to_string(),
                // "userdoc".to_string(),
                // "devdoc".to_string(),
                // "storageLayout".to_string(),
                // "irOptimized".to_string(),
            ],
        );

        // zksolc requires metadata to be 'None'
        self.project.solc_config.settings.metadata = None;

        // Update Output Selection
        self.project
            .solc_config
            .settings
            .output_selection
            .0
            .insert("*".to_string(), file_output_selection.clone());
    }
}

// Replace imports with placeholders
fn replace_imports_with_placeholders(content: String, remappings: &[RelativeRemapping]) -> String {
    let mut replaced_content = content.clone();

    // Iterate through the remappings
    for (i, remapping) in remappings.iter().enumerate() {
        // Create a placeholder based on the remapping name and index
        let placeholder = format!("REMAP_PLACEHOLDER_{}", i);

        // Define a pattern that matches the import statement, capturing the rest of the path
        let pattern = format!(r#"import\s+"{}(?P<rest>[^"]*)""#, regex::escape(&remapping.name));

        // Define a replacement that includes the placeholder and the captured rest of the path
        let replacement = format!(r#"import "{}$rest""#, placeholder);

        // Replace all instances of the pattern with the replacement
        replaced_content =
            Regex::new(&pattern).unwrap().replace_all(&replaced_content, replacement).into_owned();
    }

    replaced_content
}

fn substitute_remapped_paths(content: String, remappings: &[RelativeRemapping]) -> String {
    let mut substituted = content;

    loop {
        let mut made_replacements = false;

        for (i, r) in remappings.iter().enumerate() {
            // Create the placeholder based on the index
            let placeholder = format!("REMAP_PLACEHOLDER_{}", i);
            let import_path = r.path.path.to_str().unwrap();

            // Replace all instances of the placeholder with the remapped path
            let new_substituted = substituted.replace(&placeholder, &import_path);

            if new_substituted != substituted {
                made_replacements = true;
                substituted = new_substituted;
            }
        }

        // Exit the loop if no more replacements were made
        if !made_replacements {
            break;
        }
    }

    substituted
}
