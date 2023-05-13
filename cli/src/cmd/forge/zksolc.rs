/// This module contains the implementation of the `ZkSolc` struct and related types, which provide functionalities for compiling Solidity contracts using the zkSync compiler (`zksolc`).
/// The `ZkSolc` struct wraps the `Project` struct from the `ethers` crate, which manages the project's configuration, paths, and settings.
/// It utilizes the `solc` crate for interacting with the `zksolc` compiler and handling compilation tasks.
///
/// The `ZkSolc` struct represents a zkSync compiler instance and holds the following data:
///
/// * `project`: An instance of the `Project` struct, which manages the project's configuration, paths, and settings.
/// * `compiler_path`: The path to the `zksolc` compiler executable.
/// * `is_system`: A boolean flag indicating whether the compiler is a system-level installation.
/// * `force_evmla`: A boolean flag indicating whether to force EVMLA code generation.
/// * `standard_json`: An optional `StandardJsonCompilerInput` representing the standard JSON input for the compiler.
/// * `sources`: An optional `BTreeMap<Solc, (Version, BTreeMap<PathBuf, Source>)>` containing versioned sources for the project.
///
/// The `ZkSolc` struct provides methods for compiling Solidity contracts, handling compiler output, and configuring the compiler.
/// It utilizes the `ethers` and `solc` crates for interacting with the `zksolc` compiler and managing compilation tasks.
///
/// This module also includes the `ZkSolcOpts` struct, which represents the options for initializing a `ZkSolc` instance,
/// and the `fmt::Display` implementation for the `ZkSolc` struct, which allows displaying a human-readable representation of the `ZkSolc` instance.
use ansi_term::Colour::{Red, Yellow};
use anyhow::{anyhow, Error, Result};
use ethers::prelude::{artifacts::Source, Solc};
use ethers::solc::{
    artifacts::{output_selection::FileOutputSelection, StandardJsonCompilerInput},
    Graph, Project,
};
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
    pub compiler_path: PathBuf,
    pub is_system: bool,
    pub force_evmla: bool,
}

/// The `ZkSolc` struct represents a zkSync compiler instance and provides functionalities for compiling Solidity contracts using the `zksolc` compiler.
/// It wraps the `Project` struct from the `ethers` crate, which manages the project's configuration, paths, and settings.
///
/// The `ZkSolc` struct holds the following data:
///
/// * `project`: An instance of the `Project` struct from the `ethers` crate, which manages the project's configuration, paths, and settings.
/// * `compiler_path`: A `PathBuf` representing the path to the `zksolc` compiler executable.
/// * `is_system`: A boolean indicating whether the `zksolc` compiler is a system-level installation.
/// * `force_evmla`: A boolean indicating whether to force EVMLA code generation during compilation.
/// * `standard_json`: An optional `StandardJsonCompilerInput` representing the standard JSON input for the compiler.
/// * `sources`: An optional `BTreeMap<Solc, (Version, BTreeMap<PathBuf, Source>)>` containing versioned sources for the project.
///
/// The `ZkSolc` struct provides methods for compiling Solidity contracts, handling compiler output, and configuring the compiler.
/// It utilizes the `ethers` and `solc` crates for interacting with the `zksolc` compiler and managing compilation tasks.
///
/// The `ZkSolc` struct implements the `fmt::Display` trait to allow displaying a human-readable representation of the `ZkSolc` instance.
/// Use `println!("{}", zk_solc)` to display the `ZkSolc` instance.
///
/// # Example
///
/// ```
/// use std::path::PathBuf;
/// use ethers::solc::Project;
/// use semver::Version;
///
/// let project = Project::new("/path/to/project").unwrap();
/// let compiler_path = PathBuf::from("/path/to/zksolc");
///
/// let zk_solc_opts = ZkSolcOpts {
///     compiler_path,
///     is_system: true,
///     force_evmla: false,
/// };
///
/// let zk_solc = ZkSolc::new(zk_solc_opts, project);
///
/// println!("ZkSolc instance: {}", zk_solc);
/// ```
///
/// The example above demonstrates the usage of the `ZkSolc` struct. It creates a `Project` instance for a project located at `/path/to/project`,
/// specifies the path to the `zksolc` compiler executable, and initializes a `ZkSolc` instance using the `ZkSolcOpts` options.
/// It then displays the `ZkSolc` instance using the `fmt::Display` implementation.
// FIXME: let's add some more comments to the fields (and are you sure that all of them have to be public?)
#[derive(Debug)]
pub struct ZkSolc {
    project: Project,
    compiler_path: PathBuf,
    is_system: bool,
    force_evmla: bool,
    standard_json: Option<StandardJsonCompilerInput>,
    sources: Option<BTreeMap<Solc, (Version, BTreeMap<PathBuf, Source>)>>,
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
        }
    }

    /// Compiles the Solidity contracts in the project using the configured `zksolc` compiler.
    ///
    /// This function performs the compilation process for the Solidity contracts in the project.
    /// It configures the `zksolc` compiler, parses the JSON input for each contract, and runs the compilation command.
    /// The compiler output is then handled to display any errors or warnings, and the artifacts are saved to the appropriate files.
    ///
    /// # Errors
    ///
    /// This function can return an `Error` if any of the compilation steps fail or encounter an error.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use ethers::solc::Project;
    ///
    /// let project = Project::new("/path/to/project").unwrap();
    /// let compiler_path = PathBuf::from("/path/to/zksolc");
    ///
    /// let zk_solc_opts = ZkSolcOpts {
    ///     compiler_path,
    ///     is_system: true,
    ///     force_evmla: false,
    /// };
    ///
    /// let zk_solc = ZkSolc::new(zk_solc_opts, project);
    ///
    /// // Compile the Solidity contracts
    /// zk_solc.compile().expect("Failed to compile contracts");
    /// ```
    ///
    /// The example above demonstrates the usage of the `compile` function. It creates a `Project` instance for a project located at `/path/to/project`,
    /// specifies the path to the `zksolc` compiler executable, and initializes a `ZkSolc` instance using the `ZkSolcOpts` options.
    /// It then calls the `compile` function to perform the compilation of Solidity contracts in the project.
    pub fn compile(mut self) -> Result<()> {
        self.configure_solc();
        let sources = self.sources.clone().unwrap();
        let mut displayed_warnings = HashSet::new();
        for (solc, version) in sources {
            //configure project solc for each solc version
            for source in version.1 {
                let contract_path = source.0.clone();

                if let Err(err) = self.parse_json_input(contract_path.clone()) {
                    eprintln!("Failed to parse json input for zksolc compiler: {}", err);
                }

                let comp_args = self.build_compiler_args(source.clone(), solc.clone());

                let mut cmd = Command::new(&self.compiler_path);
                let mut child = cmd
                    .arg(contract_path.clone())
                    .args(&comp_args)
                    .stdin(Stdio::piped())
                    .stderr(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn();
                let stdin = child.as_mut().unwrap().stdin.take().expect("Stdin exists.");

                serde_json::to_writer(stdin, &self.standard_json.clone().unwrap()).map_err(
                    |e| Error::msg(format!("Could not assign standard_json to writer: {}", e)),
                )?;

                let output = child
                    .unwrap()
                    .wait_with_output()
                    .map_err(|e| Error::msg(format!("Could not run compiler cmd: {}", e)))?;

                let filename = contract_path
                    .to_str()
                    .expect("Unable to convert source to string")
                    .split(
                        self.project
                            .paths
                            .root
                            .to_str()
                            .expect("Unable to convert source to string"),
                    )
                    .nth(1)
                    .expect("Failed to get Contract relative path")
                    .split("/")
                    .last()
                    .expect("Failed to get Contract filename.");

                self.handle_output(output, filename.to_string(), &mut displayed_warnings);
            }
        }

        Ok(())
    }

    /// Builds the compiler arguments for the specified versioned source and solc version.
    ///
    /// This function constructs the compiler arguments based on the provided versioned source and solc version.
    /// It sets the necessary arguments such as `--standard-json` and `--solc`, and includes additional options
    /// like `--system-mode` and `--force-evmla` if applicable.
    ///
    /// # Parameters
    ///
    /// * `versioned_source`: A tuple containing the path to the versioned source file and the corresponding source object.
    /// * `solc`: The `Solc` object representing the solc version to be used.
    ///
    /// # Returns
    ///
    /// A vector of strings representing the compiler arguments.
    fn build_compiler_args(
        &mut self,
        versioned_source: (PathBuf, Source),
        solc: Solc,
    ) -> Vec<String> {
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

        if self.is_system || versioned_source.0.to_str().unwrap().contains("is-system") {
            comp_args.push("--system-mode".to_string());
        }

        if self.force_evmla {
            comp_args.push("--force-evmla".to_string());
        }
        comp_args
    }

    /// Handles the output of the compiler and performs necessary processing.
    ///
    /// This function takes the output of the compiler as a `std::process::Output` struct,
    /// the name of the source file, and a mutable set of displayed warnings.
    /// It processes the compiler output, extracts bytecode hashes, and handles errors and warnings.
    ///
    /// The function parses the output as JSON and checks for any errors or warnings.
    /// If errors are found, they are printed in red. If warnings are found, they are printed in yellow.
    ///
    /// Additionally, the function extracts bytecode hashes from the output and prints them.
    /// It looks for the matching source file in the output JSON and retrieves the bytecode hashes.
    /// The bytecode hashes are then printed along with their corresponding contract names.
    ///
    /// The function also writes the pretty-printed output JSON to the artifacts file.
    ///
    /// # Parameters
    ///
    /// * `output`: The output of the compiler as a `std::process::Output` struct.
    /// * `source`: The name of the source file.
    /// * `displayed_warnings`: A mutable set of displayed warnings to avoid duplicate printing.
    ///
    /// Note: This function is used internally by the `compile` function and is not intended to be called directly.
    fn handle_output(
        &self,
        output: std::process::Output,
        source: String,
        displayed_warnings: &mut HashSet<String>,
    ) {
        let output_json: Value = serde_json::from_slice(&output.clone().stdout)
            .unwrap_or_else(|e| panic!("Could not parse zksolc compiler output: {}", e));

        self.handle_output_errors(&output_json, displayed_warnings);

        let mut artifacts_file = self
            .build_artifacts_file(source.clone())
            .unwrap_or_else(|e| panic!("Error configuring solc compiler: {}", e));

        // get bytecode hash(es) to return to user
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

        let output_json_pretty = serde_json::to_string_pretty(&output_json)
            .unwrap_or_else(|e| panic!("Could not beautify zksolc compiler output: {}", e));

        artifacts_file
            .write_all(output_json_pretty.as_bytes())
            .unwrap_or_else(|e| panic!("Could not write artifacts file: {}", e));
    }

    /// Handles the errors and warnings in the compiler output.
    ///
    /// This function takes the parsed JSON output as a reference and a mutable set of displayed warnings.
    /// It processes the errors and warnings in the output and prints them in the appropriate color.
    ///
    /// The function iterates over the errors in the output JSON and extracts the severity and formatted message for each error.
    /// If the severity is "warning", it checks if the warning has already been displayed to avoid duplicates.
    /// If it's a new warning, it prints the formatted message in yellow using the `ansi_term` crate.
    ///
    /// If the severity is "error", it prints the formatted message in red using the `ansi_term` crate.
    ///
    /// The function tracks whether there are any errors or warnings and handles them accordingly.
    /// If there are errors, the function exits the program with a non-zero status code.
    /// If there are warnings, it prints a message indicating that the compilation completed with warnings.
    ///
    /// # Parameters
    ///
    /// * `output_json`: A reference to the parsed JSON output.
    /// * `displayed_warnings`: A mutable set of displayed warnings to avoid duplicate printing.
    ///
    /// Note: This function is used internally by the `handle_output` function and is not intended to be called directly.
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
            // FIXME: avoid 'exits' (that's like 'panic') - instead try to return an error -- allowing more flexibility.
            exit(1);
        } else if has_warning {
            println!("Compiler run completed with warnings");
        }
    }

    /// Parses the JSON input for the contract and performs necessary configuration.
    ///
    /// This function takes a mutable reference to `self` and the path to the contract file as a `PathBuf`.
    /// It reads the standard JSON input for the contract using the `standard_json_input` method of the project.
    /// The standard JSON input contains the necessary information for compiling the contract.
    ///
    /// The function sets the `standard_json` field of `self` to the parsed standard JSON input.
    ///
    /// Additionally, the function creates the artifacts directory and saves the JSON input to a file in the artifacts directory.
    ///
    /// # Parameters
    ///
    /// * `contract_path`: The path to the contract file as a `PathBuf`.
    ///
    /// # Errors
    ///
    /// This function can return an error if there is any issue with reading or writing files.
    ///
    /// Note: This function is used internally by the `compile` function and is not intended to be called directly.
    pub fn parse_json_input(&mut self, contract_path: PathBuf) -> Result<()> {
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

        //zksolc requires metadata to be 'None'
        self.project.solc_config.settings.metadata = None;

        self.project
            .solc_config
            .settings
            .output_selection
            .0
            .insert("*".to_string(), file_output_selection.clone());

        let standard_json = self
            .project
            .standard_json_input(&contract_path)
            .map_err(|e| Error::msg(format!("Could not get standard json input: {}", e)))
            .unwrap();
        self.standard_json = Some(standard_json.to_owned());

        let artifact_path = &self
            .build_artifacts_path(contract_path)
            .map_err(|e| Error::msg(format!("Could not build_artifacts_path: {}", e)))
            .unwrap();

        let path = artifact_path.join("json_input.json");
        match File::create(&path) {
            Err(why) => panic!("couldn't create : {}", why),
            Ok(file) => file,
        };
        // Save the JSON input to build folder.
        let stdjson = serde_json::to_value(&standard_json).unwrap();
        std::fs::write(path, serde_json::to_string_pretty(&stdjson).unwrap()).unwrap();

        Ok(())
    }

    fn configure_solc(&mut self) {
        self.sources = Some(self.get_versioned_sources().unwrap());
    }

    /// Retrieves the versioned sources for the project.
    ///
    /// This function retrieves the versioned sources for the project by resolving the project sources using a graph.
    /// It first retrieves the sources using the `sources` method of the project.
    /// Then, it creates a graph and resolves the sources by calling `Graph::resolve_sources` with the project paths and sources.
    ///
    /// The function returns a `Result` containing a `BTreeMap` of solc versions to a tuple of version and source map.
    /// The source map is a `BTreeMap` where the keys are paths to source files and the values are `Source` objects.
    ///
    /// # Errors
    ///
    /// This function can return an error if there is any issue with retrieving or resolving the project sources.
    ///
    /// # Example
    ///
    /// ```rust
    /// let versioned_sources = zk_solc.get_versioned_sources()?;
    /// ```
    ///
    /// The example above demonstrates the usage of the `get_versioned_sources` function.
    /// It calls the `get_versioned_sources` function to retrieve the versioned sources for the project.
    ///
    /// Note: This function is used internally by the `configure_solc` function and is not intended to be called directly.
    fn get_versioned_sources(
        &mut self,
    ) -> Result<BTreeMap<Solc, (Version, BTreeMap<PathBuf, Source>)>> {
        let sources = self
            .project
            .sources()
            .map_err(|e| Error::msg(format!("Could not get project sources: {}", e)))
            .unwrap();

        let graph = Graph::resolve_sources(&self.project.paths, sources)
            .map_err(|e| Error::msg(format!("Could not create graph: {}", e)))
            .unwrap();

        let (versions, _edges) = graph
            .into_sources_by_version(self.project.offline)
            .map_err(|e| Error::msg(format!("Could not get versions & edges: {}", e)))
            .unwrap();

        let solc_version = versions
            .get(&self.project)
            .map_err(|e| Error::msg(format!("Could not get solc: {}", e)));

        solc_version
    }

    /// Builds the artifacts path for a given source file.
    ///
    /// This function takes a source file path and constructs the artifacts path where the compiled artifacts
    /// will be stored. It appends the filename of the source file to the artifacts directory and creates the
    /// directory structure if it doesn't exist.
    ///
    /// # Arguments
    ///
    /// * `source`: A `PathBuf` representing the path to the source file.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `PathBuf` of the artifacts path if successful, or an `anyhow::Error` if there
    /// is an issue creating the artifacts directory.
    fn build_artifacts_path(&self, source: PathBuf) -> Result<PathBuf, anyhow::Error> {
        // FIXME: return error with anyhow! rather than failing (with expect)
        let filename = source.file_name().expect("Failed to get Contract filename.");

        let path = self.project.paths.artifacts.join(filename);
        fs::create_dir_all(&path)
            .map_err(|e| Error::msg(format!("Could not create artifacts directory: {}", e)))?;
        Ok(path)
    }

    /// Builds the artifacts file for a given source file.
    ///
    /// This function takes a source file path and constructs the path to the artifacts file where the compiled
    /// artifacts will be stored. It appends the source file path to the artifacts directory and creates the file.
    ///
    /// # Arguments
    ///
    /// * `source`: A `String` representing the source file path.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `File` object representing the artifacts file if successful,
    /// or an `anyhow::Error` if there is an issue creating the artifacts file.
    fn build_artifacts_file(&self, source: String) -> Result<File> {
        // FIXME: No need  for this local variable
        let artifacts_file =
            File::create(self.project.paths.artifacts.join(source).join("artifacts.json"))
                .map_err(|e| Error::msg(format!("Could not create artifacts file: {}", e)))?;

        Ok(artifacts_file)
    }
}
