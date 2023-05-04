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

use ethers::prelude::artifacts::Source;
use ethers::prelude::Solc;
use semver::Version;

#[derive(Debug, Clone)]
pub struct ZkSolcOpts {
    pub compiler_path: PathBuf,
    pub is_system: bool,
    pub force_evmla: bool,
}

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

impl<'a> ZkSolc {
    pub fn new(opts: ZkSolcOpts, mut project: Project) -> Self {
        Self {
            project,
            compiler_path: opts.compiler_path,
            is_system: opts.is_system,
            force_evmla: opts.force_evmla,
            standard_json: None,
            sources: None,
        }
    }

    pub fn compile(mut self) -> Result<()> {
        // let comp_args = self.build_compiler_args();
        self.configure_solc();
        let sources = self.sources.clone().unwrap();
        for (solc, version) in sources {
            //configure project solc for each solc version
            for source in version.1 {
                let contract_path = source.0.clone();
                if let Err(err) = self.parse_json_input(contract_path.clone()) {
                    eprintln!("Failed to parse json input for zksolc compiler: {}", err);
                }

                //start thinking about contrract specific parameters like is-system flag and maybe solc versions
                // println!("{:#?}, solc", solc);

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

                let source_str = contract_path
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
                    .unwrap()
                    .split("/")
                    .last()
                    .unwrap();

                self.write_artifacts(output, source_str.to_string());
            }
        }

        Ok(())
    }

    //issues with async functions
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

        // if self.is_system || source.to_str().unwrap().contains("is-system") {
        //     println!("{:#?}, is system source", source);
        //     comp_args.push("--system-mode".to_string());
        // }

        if self.force_evmla {
            comp_args.push("--force-evmla".to_string());
        }
        comp_args
    }

    fn write_artifacts(&self, output: std::process::Output, source: String) {
        let mut artifacts_file = self
            .build_artifacts_file(source.clone())
            .unwrap_or_else(|e| panic!("Error configuring solc compiler: {}", e));

        let output_json: Value = serde_json::from_slice(&output.clone().stdout)
            .unwrap_or_else(|e| panic!("Could not parse zksolc compiler output: {}", e));

        // get bytecode hash(es) to return to user
        let output_obj = output_json["contracts"].as_object().unwrap();
        for key in output_obj.keys() {
            if key.contains(&source) {
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
        // println!("{:#?}, solc_version", solc_version);
        // println!("{:#?}, solc_version length", solc_version.len());

        // if let Some(solc_first_key) = solc_version.first_key_value() {
        //     // TODO: understand and handle solc versions and the edge cases here

        //     Ok(solc_first_key.0.solc.to_owned())
        // } else {
        //     Err(Error::msg("Could not get solc path"))
        // }
    }

    fn get_versioned_sources(
        &mut self,
    ) -> Result<BTreeMap<Solc, (Version, BTreeMap<PathBuf, Source>)>> {
        let sources = self
            .project
            .sources()
            .map_err(|e| Error::msg(format!("Could not get project sources: {}", e)))
            .unwrap();

        // let s = sources.clone();
        // let keys = s.into_keys();
        // let vec: Vec<PathBuf> = keys.collect();
        // self.sources = Some(vec);

        let graph = Graph::resolve_sources(&self.project.paths, sources)
            .map_err(|e| Error::msg(format!("Could not create graph: {}", e)))
            .unwrap();

        let (versions, _edges) = graph
            .into_sources_by_version(self.project.offline)
            .map_err(|e| Error::msg(format!("Could not get versions & edges: {}", e)))
            .unwrap();
        println!("{:#?}, versions", versions);
        let solc_version = versions
            .get(&self.project)
            .map_err(|e| Error::msg(format!("Could not get solc: {}", e)));
        println!("{:#?}, solc_version", solc_version);
        solc_version
    }

    fn build_artifacts_path(&self, source: PathBuf) -> Result<PathBuf, anyhow::Error> {
        let source_str = source
            .to_str()
            .expect("Unable to convert source to string")
            .split(self.project.paths.root.to_str().expect("Unable to convert source to string"))
            .nth(1)
            .unwrap()
            .split("/")
            .last()
            .unwrap();

        let path = self.project.paths.artifacts.join(source_str);
        fs::create_dir_all(&path)
            .map_err(|e| Error::msg(format!("Could not create artifacts directory: {}", e)))?;
        Ok(path)
    }

    fn build_artifacts_file(&self, source: String) -> Result<File> {
        let artifacts_file =
            File::create(self.project.paths.artifacts.join(source).join("artifacts.json"))
                .map_err(|e| Error::msg(format!("Could not create artifacts file: {}", e)))?;

        Ok(artifacts_file)
    }
}
