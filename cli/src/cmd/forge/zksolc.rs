use anyhow::{Error, Result};
use ethers::solc::{
    artifacts::{output_selection::FileOutputSelection, StandardJsonCompilerInput},
    Graph, Project,
};

use serde_json::Value;
use std::path::PathBuf;
use std::{
    collections::BTreeMap,
    fmt, fs,
    fs::File,
    io::Write,
    process::{Command, Stdio},
};

#[derive(Debug, Clone)]
pub struct ZkSolcOpts {
    pub compiler_path: PathBuf,
    pub contract_name: String,
    pub is_system: bool,
    pub force_evmla: bool,
}

#[derive(Debug)]
pub struct ZkSolc {
    pub project: Project,
    pub compiler_path: PathBuf,
    pub contracts_path: PathBuf,
    pub artifacts_path: PathBuf,
    pub is_system: bool,
    pub force_evmla: bool,
    pub standard_json: Option<StandardJsonCompilerInput>,
}

impl fmt::Display for ZkSolc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ZkSolc (
                compiler_path: {},   
                output_path: {},
                contracts_path: {},
                artifacts_path: {}, 
            )",
            self.compiler_path.display(),
            self.project.paths.artifacts.display(),
            self.contracts_path.display(),
            self.artifacts_path.display(),
        )
    }
}

impl<'a> ZkSolc {
    pub fn new(opts: ZkSolcOpts, mut project: Project) -> Self {
        let zk_out_path = project.paths.root.to_owned().join("zkout");
        let contracts_path = project.paths.sources.to_owned().join(opts.contract_name.clone());
        let artifacts_path = zk_out_path.to_owned().join(opts.contract_name.clone());
        project.paths.artifacts = zk_out_path;

        Self {
            project,
            compiler_path: opts.compiler_path,
            contracts_path,
            artifacts_path,
            is_system: opts.is_system,
            force_evmla: opts.force_evmla,
            standard_json: None,
        }
    }

    pub fn compile(self) -> Result<()> {
        let (contract_path, comp_args) = self.build_compiler_args();
        let mut cmd = Command::new(&self.compiler_path);
        let mut child = cmd
            .arg(contract_path)
            .args(comp_args)
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn();
        let stdin = child.as_mut().unwrap().stdin.take().expect("Stdin exists.");

        serde_json::to_writer(stdin, &self.standard_json.clone().unwrap())
            .map_err(|e| Error::msg(format!("Could not assign standard_json to writer: {}", e)))?;

        let output = child
            .unwrap()
            .wait_with_output()
            .map_err(|e| Error::msg(format!("Could not run compiler cmd: {}", e)))?;

        self.write_artifacts(output);

        Ok(())
    }

    fn build_compiler_args(&self) -> (String, Vec<String>) {
        let solc_path = self
            .configure_solc()
            .unwrap_or_else(|e| panic!("Could not configure solc: {}", e))
            .to_str()
            .unwrap_or_else(|| panic!("Error configuring solc compiler."))
            .to_string();

        let contracts_path = self
            .contracts_path
            .to_str()
            .unwrap_or_else(|| panic!("No contracts path found."))
            .to_string();

        // Build compiler arguments
        let mut comp_args = Vec::<String>::new();
        comp_args.push("--standard-json".to_string());
        comp_args.push("--solc".to_string());
        comp_args.push(solc_path.to_owned());

        if self.is_system {
            comp_args.push("--system-mode".to_string());
        }

        if self.force_evmla {
            comp_args.push("--force-evmla".to_string());
        }
        (contracts_path, comp_args)
    }

    fn write_artifacts(&self, output: std::process::Output) {
        let mut artifacts_file = self
            .build_artifacts_file()
            .unwrap_or_else(|e| panic!("Error configuring solc compiler: {}", e));

        let output_json: Value = serde_json::from_slice(&output.clone().stdout)
            .unwrap_or_else(|e| panic!("Could to parse zksolc compiler output: {}", e));

        // get bytecode hash(es) to return to user
        let output_obj = output_json["contracts"].as_object().unwrap();
        let keys = output_obj.keys();
        let ctx_filename = self.contracts_path.to_str().unwrap().split("/").last().unwrap();
        for key in keys {
            if key.contains(ctx_filename) {
                let b_code = output_obj[key].clone();
                let b_code_obj = b_code.as_object().unwrap();
                let b_code_keys = b_code_obj.keys();
                for hash in b_code_keys {
                    let bcode_hash = b_code_obj[hash]["hash"].clone();
                    println!("{}", format!("{} -> Bytecode Hash: {} ", hash, bcode_hash));
                }
            }
        }

        let output_json_pretty = serde_json::to_string_pretty(&output_json)
            .unwrap_or_else(|e| panic!("Could not beautify zksolc compiler output: {}", e));

        artifacts_file
            .write_all(output_json_pretty.as_bytes())
            .unwrap_or_else(|e| panic!("Could not write artifacts file: {}", e));
    }

    pub fn parse_json_input(&mut self) -> Result<()> {
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
                // "ast".to_string(),
                "metadata".to_string(),
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
            .standard_json_input(&self.contracts_path)
            .map_err(|e| Error::msg(format!("Could not get standard json input: {}", e)))?;
        self.standard_json = Some(standard_json.to_owned());

        let _ = &self
            .build_artifacts_path()
            .map_err(|e| Error::msg(format!("Could not build_artifacts_path: {}", e)))?;

        let path = self.artifacts_path.join("json_input.json");
        match File::create(&path) {
            Err(why) => panic!("couldn't create : {}", why),
            Ok(file) => file,
        };
        // Save the JSON input to build folder.
        let stdjson = serde_json::to_value(&standard_json).unwrap();
        std::fs::write(path, serde_json::to_string_pretty(&stdjson).unwrap()).unwrap();

        Ok(())
    }

    fn configure_solc(&self) -> Result<PathBuf> {
        let sources = self
            .project
            .sources()
            .map_err(|e| Error::msg(format!("Could not get project sources: {}", e)))?;

        let graph = Graph::resolve_sources(&self.project.paths, sources)
            .map_err(|e| Error::msg(format!("Could not create graph: {}", e)))?;

        let (versions, _edges) = graph
            .into_sources_by_version(self.project.offline)
            .map_err(|e| Error::msg(format!("Could not get versions & edges: {}", e)))?;

        let solc_version = versions
            .get(&self.project)
            .map_err(|e| Error::msg(format!("Could not get solc: {}", e)))?;

        if let Some(solc_first_key) = &solc_version.first_key_value() {
            // TODO: understand and handle solc versions and the edge cases here

            Ok(solc_first_key.0.solc.to_owned())
        } else {
            Err(Error::msg("Could not get solc path"))
        }
    }

    fn build_artifacts_path(&self) -> Result<(), anyhow::Error> {
        fs::create_dir_all(&self.artifacts_path)
            .map_err(|e| Error::msg(format!("Could not create artifacts directory: {}", e)))?;
        Ok(())
    }

    fn build_artifacts_file(&self) -> Result<File> {
        let artifacts_file = File::create(self.artifacts_path.join("artifacts.json"))
            .map_err(|e| Error::msg(format!("Could not create artifacts file: {}", e)))?;

        Ok(artifacts_file)
    }
}
