use ansi_term::Colour::{Red, Yellow};
use anyhow::{anyhow, Error, Result};
use ethers::solc::{
    artifacts::{output_selection::FileOutputSelection, StandardJsonCompilerInput},
    Graph, Project,
};
use serde_json::Value;
use std::path::PathBuf;
use std::{
    collections::{BTreeMap, HashSet},
    fmt, fs,
    fs::File,
    io::Write,
    process::{exit, Command, Stdio},
};

use ethers::prelude::artifacts::Source;
use ethers::prelude::Solc;
use semver::Version;

#[derive(Debug, Clone)]
pub struct ZkSolcOpts {
    pub compiler_path: PathBuf,
    pub is_system: bool,
    pub force_evmla: bool,
}

// FIXME: let's add some more comments to the fields (and are you sure that all of them have to be public?)
#[derive(Debug)]
pub struct ZkSolc {
    pub project: Project,
    pub compiler_path: PathBuf,
    pub is_system: bool,
    pub force_evmla: bool,
    pub standard_json: Option<StandardJsonCompilerInput>,
    pub sources: Option<BTreeMap<Solc, (Version, BTreeMap<PathBuf, Source>)>>,
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

    //FIXME:  'mut self' without reference is quite strange.. are you sure?
    pub fn compile(mut self) -> Result<()> {
        self.configure_solc();
        //FIXME: Check your clones - you can probably do just a reference..
        // also - when you're already returning a result - maybe don't unwrap but '?' (with map_err to add more info if needed).
        //  also - 'as_ref' allows you to 'access the reference' of the other object.
        let sources = self.sources.clone().unwrap();
        // let sources =
        //     self.sources.as_ref().ok_or(anyhow!("Missing sources?? TODO: what does it mean?"))?;
        let mut displayed_warnings = HashSet::new();
        for (solc, version) in sources {
            //configure project solc for each solc version
            // FIXME: and with the iterator - you can get access to the reference (so no copying needed)
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

    fn build_compiler_args(
        &mut self,
        versioned_source: (PathBuf, Source),
        solc: Solc,
    ) -> Vec<String> {
        // FIXME: this is a 'path' - so you can use 'Path' object (rather than str)
        let solc_path = solc
            .solc
            .to_str()
            .unwrap_or_else(|| panic!("Error configuring solc compiler."))
            .to_string();

        // Build compiler arguments
        // FIXME: you can use 'vec!'
        let mut comp_args = Vec::<String>::new();
        comp_args.push("--standard-json".to_string());
        comp_args.push("--solc".to_string());
        comp_args.push(solc_path.to_owned());
        // let mut comp_args = vec!["--stardard-json".to_string(), "--solc".to_string(), solc_path];

        if self.is_system || versioned_source.0.to_str().unwrap().contains("is-system") {
            comp_args.push("--system-mode".to_string());
        }

        if self.force_evmla {
            comp_args.push("--force-evmla".to_string());
        }
        comp_args
    }

    // FIXME: please add comments to some functions.
    // Handles compiler output
    // Artifacts
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

    fn build_artifacts_path(&self, source: PathBuf) -> Result<PathBuf, anyhow::Error> {
        // FIXME: return error with anyhow! rather than failing (with expect)
        let filename = source.file_name().expect("Failed to get Contract filename.");

        let path = self.project.paths.artifacts.join(filename);
        fs::create_dir_all(&path)
            .map_err(|e| Error::msg(format!("Could not create artifacts directory: {}", e)))?;
        Ok(path)
    }

    fn build_artifacts_file(&self, source: String) -> Result<File> {
        // FIXME: No need  for this local variable
        let artifacts_file =
            File::create(self.project.paths.artifacts.join(source).join("artifacts.json"))
                .map_err(|e| Error::msg(format!("Could not create artifacts file: {}", e)))?;

        Ok(artifacts_file)
    }
}
