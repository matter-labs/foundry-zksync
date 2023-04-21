use anyhow::{Error, Result};
use ethers::solc::{
    artifacts::{output_selection::FileOutputSelection, StandardJsonCompilerInput},
    Graph, Project,
};
use foundry_config::Config;
use serde:: Serialize;
use serde_json::{Value};
use std::path::{self, PathBuf};
use std::{
    collections::BTreeMap,
    fmt, fs,
    fs::File,
    io::Write,
    process::{Command, Stdio},
};

#[derive(Debug, Clone)]
pub struct ZkSolcOpts<'a> {
    pub config: &'a Config,
    pub project: &'a Project,
    pub compiler_path: PathBuf,
    pub contract_name: String,
    // pub contracts_path: PathBuf,
    // pub is_system: bool,
    // pub force_evmla: bool,
}

// impl<'a> Default for ZkSolcOpts<'a> {
//     fn default() -> Self {
//         Self {
//             config: &Config::default(),
//             project: &Project::default(),
//             compiler_path: PathBuf::new(),
//             is_system: false,
//             force_evmla: false,
//         }
//     }
// }

#[derive(Debug, Clone)]
pub struct ZkSolc<'a> {
    // pub config: &'a Config,
    pub project: &'a Project,
    pub compiler_path: PathBuf,
    pub output_path: PathBuf,
    pub contracts_path: PathBuf,
    pub artifacts_path: PathBuf,
    // pub is_system: bool,
    // pub force_evmla: bool,
    pub standard_json: Option<StandardJsonCompilerInput>,
}

impl fmt::Display for ZkSolc<'_> {
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
            self.output_path.display(),
            self.contracts_path.display(),
            self.artifacts_path.display(),
        )
    }
}

impl<'a> ZkSolc<'a> {
    pub fn new(opts: ZkSolcOpts<'a>) -> Self {
        // let mut project = Config::load().project().unwrap();
        // let contracts_path = opts.project.paths.sources.to_owned().join(opts.contract_name.clone());

        Self {
            // config: todo!(),
            project: opts.project,
            compiler_path: opts.compiler_path,
            output_path: opts.project.paths.root.to_owned().join("zkout"),
            contracts_path: opts.project.paths.sources.to_owned().join(opts.contract_name.clone()),
            artifacts_path: opts
                .project
                .paths
                .root
                .to_owned()
                .join("zkout")
                .join(opts.contract_name.clone()),
            // is_system: todo!(),
            // force_evmla: todo!(),
            standard_json: None,
        }
    }

    // TODO: hander errs instead of unwraps
    pub fn compile(&self) -> Result<()> {
        self.clone()
            .build_artifacts_path()
            .map_err(|e| Error::msg(format!("Could not build_artifacts_path: {}", e)))?;

        let solc_path = self
            .clone()
            .configure_solc()
            .map_err(|e| Error::msg(format!("Could not configure solc: {}", e)))?;

        // TODO: configure vars appropriately, this is a happy path to compilation
        let mut comp_args: Vec<String> =
            vec![self.clone().contracts_path.into_os_string().into_string().unwrap()];
        comp_args.push("--solc".to_string());
        comp_args.push(solc_path.clone().into_os_string().into_string().unwrap());
        // comp_args.push("--system-mode".to_string());
        // comp_args.push("--force-evmla".to_string());
        // comp_args.push("--bin".to_string());
        // comp_args.push("--combined-json".to_string());
        // comp_args.push("abi,bin,hashes".to_string());
        comp_args.push("--standard-json".to_string());

        let mut cmd = Command::new(self.clone().compiler_path);
        let mut child = cmd
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

        let mut artifacts_file = self
            .clone()
            .build_artifacts_file()
            .map_err(|e| Error::msg(format!("Could create artifacts file: {}", e)))?;

        let output_json: Value = serde_json::from_slice(&output.clone().stdout)
            .map_err(|e| Error::msg(format!("Could to parse zksolc compiler output: {}", e)))?;

        let output_json_pretty = serde_json::to_string_pretty(&output_json)
            .map_err(|e| Error::msg(format!("Could to beautify zksolc compiler output: {}", e)))?;

        artifacts_file
            .write_all(output_json_pretty.as_bytes())
            .map_err(|e| Error::msg(format!("Could not write artifacts file: {}", e)))?;

        Ok(())
    }

    pub fn parse_json_input(&mut self) -> Result<()> {
        let mut project = Config::load().project().unwrap();

        let mut file_output_selection: FileOutputSelection = BTreeMap::default();
        file_output_selection.insert(
            "*".to_string(),
            vec![
                "abi".to_string(),
                "evm.methodIdentifiers".to_string(),
                "evm.legacyAssembly".to_string(),
            ],
        );
        file_output_selection.insert(
            "".to_string(),
            vec![
                "ast".to_string(),
                "metadata".to_string(),
                "userdoc".to_string(),
                "devdoc".to_string(),
                "storageLayout".to_string(),
                "irOptimized".to_string(),
            ],
        );

        project
            .solc_config
            .settings
            .output_selection
            .0
            .insert("*".to_string(), file_output_selection.clone());

        let standard_json = project
            .standard_json_input(&self.contracts_path)
            .map_err(|e| Error::msg(format!("Could not get standard json input: {}", e)))?;
        self.standard_json = Some(standard_json.to_owned());

        let stdjson = serde_json::to_value(&standard_json)
            .map_err(|e| Error::msg(format!("Could not parse standard json input: {}", e)))?;

        let json_input = self.artifacts_path.join("json_input.json");

        let _file = File::create(&json_input)
            .map_err(|e| Error::msg(format!("Could create input json file: {}", e)))?;

        let file_contents = serde_json::to_string_pretty(&stdjson)
            .map_err(|e| Error::msg(format!("Could parse input json file contents: {}", e)))?;

        let _io_result = std::fs::write(json_input, file_contents).map_err(|e| {
            Error::msg(format!("Could not write input json contents to file: {}", e))
        })?;
        Ok(())
    }

    fn configure_solc(self) -> Result<PathBuf> {
        let sources = self
            .project
            .sources()
            .map_err(|e| Error::msg(format!("Could not get project sources: {}", e)))?;

        let graph = Graph::resolve_sources(&self.project.paths, sources)
            .map_err(|e| Error::msg(format!("Could not create graph: {}", e)))?;

        let (versions, edges) = graph
            .into_sources_by_version(self.project.offline)
            .map_err(|e| Error::msg(format!("Could not get versions & edges: {}", e)))?;

        let solc_version = versions
            .get(&self.project)
            .map_err(|e| Error::msg(format!("Could not get solc: {}", e)))?;

        println!("+++++++++++++++++++++++++++");
        // println!("{:?}", solc_version);
        println!("+++++++++++++++++++++++++++");

        if let Some(solc_first_key) = &solc_version.first_key_value() {
            // TODO: understand and handle solc versions and the edge cases here

            Ok(solc_first_key.0.solc.to_owned())
        } else {
            Err(Error::msg("Could not get solc path"))
        }
    }

    // fn build_artifacts_path(mut self) -> Result<()> {
    //     let artifacts_path = self.output_path.join("artifacts.json");
    //     self.artifacts_path = artifacts_path;

    //     Ok(())
    // }

    fn build_artifacts_path(mut self) -> Result<()> {
        match fs::create_dir_all(&self.artifacts_path) {
            Ok(()) => println!(" create build_path folder success"),
            Err(error) => panic!("problem creating build_path folder: {:#?}", error),
        };
        Ok(())
    }

    fn build_artifacts_file(self) -> Result<File> {
        let artifacts_file = File::create(&self.artifacts_path.join("artifacts.json"))
            .map_err(|e| Error::msg(format!("Could not create artifacts file: {}", e)))?;

        Ok(artifacts_file)
    }
}
