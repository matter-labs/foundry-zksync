use color_eyre::Report;
use ethers::solc::artifacts::output_selection::{FileOutputSelection, OutputSelection};
use ethers::solc::error::SolcError;
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
use std::process::{Command, Stdio};

use super::utils_zksync;

//if we need to ultimately return something
// pub fn compile_zksync(config: &Config) -> eyre::Result<ProjectCompileOutput> {
pub fn compile_zksync(config: &Config, contract_path: &String) {
    let mut project = config.project().unwrap();
    project.auto_detect = false;
    let contract_full_path = &format!("{}/{}", project.paths.sources.display(), contract_path);
    let zkout_path = &format!("{}{}", project.paths.artifacts.display(), "/zksolc");
    let build_path = &format!("{}/{}", zkout_path, contract_path);
    let standard_json_path = &format!("{}/std_json_out", zkout_path);

    match fs::create_dir_all(std::path::Path::new(zkout_path)) {
        Ok(()) => println!(" create zkout folder success"),
        Err(error) => panic!("problem creating zkout folder: {:#?}", error),
    };

    match fs::create_dir_all(std::path::Path::new(standard_json_path)) {
        Ok(()) => println!(" create standard_json_path folder success"),
        Err(error) => panic!("problem creating standard_json_path folder: {:#?}", error),
    };

    match fs::create_dir_all(std::path::Path::new(build_path)) {
        Ok(()) => println!(" create build_path folder success"),
        Err(error) => panic!("problem creating build_path folder: {:#?}", error),
    };

    //check for compiler
    let zksolc_path = &format!("{}{}", zkout_path, "/zksolc-linux-amd64-musl-v1.3.7");
    let b = std::path::Path::new(zksolc_path).exists();

    if !b {
        utils_zksync::download_zksolc_compiler(zksolc_path, zkout_path);
    }

    let mut file_output_selection: FileOutputSelection = BTreeMap::default();
    file_output_selection
        .insert("*".to_string(), vec!["abi".to_string(), "evm.methodIdentifiers".to_string()]);
    file_output_selection.insert("".to_string(), vec!["ast".to_string()]);

    project
        .solc_config
        .settings
        .output_selection
        .0
        .insert("*".to_string(), file_output_selection.clone());

    // let sources = project.sources().unwrap();
    // println!("{:#?} project solc", project.solc);
    println!("{:#?} project.solc_config", project.solc_config);
    // println!("{:#?} project", &project);
    // println!("{:#?} config", &config);

    let standard_json = project.standard_json_input(contract_full_path).unwrap();
    println!("{:#?} standard_json", standard_json);

    // Save the JSON structure into the other file.
    let stdjson = serde_json::to_value(&standard_json).unwrap();
    match File::create("json_input.json") {
        Err(why) => panic!("couldn't create : {}", why),
        Ok(file) => file,
    };
    std::fs::write("json_input.json", serde_json::to_string_pretty(&stdjson).unwrap()).unwrap();

    // compile project
    let mut cmd = Command::new(zksolc_path);
    let mut child = cmd
        .args([contract_full_path, "--standard-json"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn();
    let stdin = child.as_mut().unwrap().stdin.take().expect("Stdin exists.");
    serde_json::to_writer(stdin, &standard_json).unwrap();
    let output = child.unwrap().wait_with_output();
    // println!("{:#?}, output", output);

    let path = &format!("{}/{}", build_path, "artifacts.json");
    let path = Path::new(path);
    let display = path.display();
    let mut file = match File::create(path) {
        Err(e) => panic!("couldn't create {}: {}", display, e),
        Ok(file) => file,
    };

    file.write_all(&output.unwrap().stdout).unwrap();

    // Below is an alternative approach to compiling using --combine-json
    // //-------------------------------------------//
    // // THIS is all working for combined-json output
    // //compile using combined-json flag
    // let output = Command::new(zksolc_path)
    //     .args([
    //         contract_full_path,
    //         "--combined-json",
    //         "abi,bin,hashes",
    //         "--output-dir",
    //         build_path,
    //         "--overwrite",
    //     ])
    //     .output()
    //     .expect("failed to execute process");

    // println!("{:#?} output", &output);
    // ----------------------------------------------//

    // return compile_out;
}
