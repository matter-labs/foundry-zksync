use color_eyre::Report;
use ethers::solc::artifacts::output_selection::{FileOutputSelection, OutputSelection};
use ethers::solc::{
    CompilerInput, ConfigurableContractArtifact, Project, ProjectCompileOutput, Solc, SolcConfig,
};
use foundry_common::compile::ProjectCompiler;
use foundry_config::Config;
use serde_json::{json, Result};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::Command;

use super::utils_zksync;

//if we need to ultimately return something
// pub fn compile_zksync(config: &Config) -> eyre::Result<ProjectCompileOutput> {
pub fn compile_zksync(config: &Config, contract_path: &String) {
    let project = config.project().unwrap();
    let contract_full_path = &format!("{}/{}", project.paths.sources.display(), contract_path);
    let zkout_path = &format!("{}{}", project.paths.artifacts.display(), "/zksolc");
    let build_path = &format!("{}/{}", zkout_path, contract_path);

    match fs::create_dir_all(std::path::Path::new(zkout_path)) {
        Ok(success) => println!("{:#?}, create zkout folder success", success),
        Err(error) => panic!("problem creating zkout folder: {:#?}", error),
    };

    //check for compiler
    let zksolc_path = &format!("{}{}", zkout_path, "/zksolc-linux-amd64-musl-v1.3.7");
    let b = std::path::Path::new(zksolc_path).exists();

    if !b {
        utils_zksync::download_zksolc_compiler(zksolc_path, zkout_path);
    }

    //-------------------------------------------//
    // THIS is all working for combined-json output
    //compile using combined-json flag
    let output = Command::new(zksolc_path)
        .args([
            contract_full_path,
            // "--optimize",
            "--combined-json",
            "abi,bin,hashes",
            "--output-dir",
            build_path,
            "--overwrite",
        ])
        // .stdin(Stdio::piped())
        .output()
        .expect("failed to execute process");

    // let path = &format!("{}/{}", artifacts_path, "artifacts1.json");
    // let path = Path::new(path);
    // let display = path.display();

    // let mut file = match File::create(path) {
    //     Err(why) => panic!("couldn't create {}: {}", display, why),
    //     Ok(file) => file,
    // };

    // file.write_all(&output.stdout).unwrap();

    // println!("{:#?} output", &output);
    // println!("{:#?} project", &project);
    // println!("{:#?} config", &config);
    // ----------------------------------------------//

    // Below is an alternative approach to compiling

    // // let output_settings = OutputSelection::complete_output_selection();
    // let output_settings = OutputSelection::default();
    // let mut file_output_selection: FileOutputSelection = BTreeMap::default();
    // file_output_selection.insert(
    //     "*".to_string(),
    //     vec![
    //         // "abi".to_string(),
    //         "bin".to_string(),
    //         // "metadata".to_string(),
    //         // "evm.bytecode".to_string(),
    //     ],
    // );
    // file_output_selection.insert(
    //     "".to_string(),
    //     vec![
    //         // "ast".to_string(),
    //         // "evm.deployedBytecode".to_string(),
    //         "evm.methodIdentifiers".to_string(),
    //     ],
    // );

    // // let sources = project.sources().unwrap();
    // let mut solc_config = SolcConfig::builder().build();
    // solc_config.settings.output_selection = output_settings.clone();
    // solc_config.settings.output_selection.0.insert("*".to_string(), file_output_selection.clone());
    // project.solc_config = solc_config.clone();

    // project.solc = Solc::new(zksolc_path).args([
    //     // "--abi",
    //     "--bin",
    //     // "--hashes",
    //     "--optimize",
    //     "--standard-json",
    // ]);

    // println!("{:#?} project solc", project.solc);
    // println!("{:#?} project.solc_config", project.solc_config);

    // let zk_compiler = ProjectCompiler::new(true, true);
    // let compile_out = zk_compiler.compile(&project);
    // println!("{:#?} compile_out", compile_out);

    // let mut standard_json = project.standard_json_input(contract_full_path).unwrap();
    // println!("{:#?} standard_json", standard_json.settings);
    // let stdjson = serde_json::to_value(&standard_json).unwrap();

    // let mut file = match File::create("json.json") {
    //     Err(why) => panic!("couldn't create : {}", why),
    //     Ok(file) => file,
    // };

    // Save the JSON structure into the other file.
    // std::fs::write(zk_json_path, serde_json::to_string_pretty(&stdjson).unwrap()).unwrap();

    //--------------------------------//

    // let compile_json = project.compile().unwrap();
    // let artifacts = compile_json.clone().into_artifacts();

    // for (id, artifact) in artifacts {
    //     let name = id.name;
    //     let abi = artifact.clone().abi.unwrap();
    //     // println!("{:#?} abi", abi);
    //     println!("CONTRACT: {:#?}", name);
    //     // println!("ARTIFACT: {:#?}, artifact", artifact);

    //     let path = &format!("{}/{}.json", zk_abi_path, name);
    //     let path = Path::new(path);
    //     let display = path.display();

    //     let mut file = match File::create(path) {
    //         Err(why) => panic!("couldn't create {}: {}", display, why),
    //         Ok(file) => file,
    //     };

    //     std::fs::write(path, serde_json::to_string(&artifact).unwrap()).unwrap();
    // }
    //--------------------------------//

    // println!("{:#?} compile_json", compile_json);
    // let artifacts_with_files: Vec<(String, String, ConfigurableContractArtifact)> =
    //     compile_json.clone().into_artifacts_with_files().collect();
    // for (file, name, artifact) in artifacts_with_files {
    //     println!("FILE: {:#?}", file);
    //     println!("NAME: {:#?}", name);
    //     // println!("ARTIFACT: {:#?}, artifact abi", artifact.abi);
    //     println!("bytecode: {:#?}, artifact", artifact);
    // }

    // let contracts = compile_json.clone().output();
    // println!("{:#?} contracts.contracts", contracts.contracts);

    // let path = &format!("{}/{}", zkout_path, "compiler_output.json");
    // let path = Path::new(path);
    // let display = path.display();

    // let mut file = match File::create(path) {
    //     Err(why) => panic!("couldn't create {}: {}", display, why),
    //     Ok(file) => file,
    // };

    // std::fs::write(path, serde_json::to_string_pretty(&contracts.contracts).unwrap()).unwrap();

    // file.write_all(contracts.contracts).unwrap();

    // return compile_out;
}
