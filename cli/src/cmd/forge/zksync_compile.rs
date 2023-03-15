use color_eyre::Report;
use ethers::solc::artifacts::output_selection::{FileOutputSelection, OutputSelection};
use ethers::solc::{CompilerInput, ProjectCompileOutput, Solc, SolcConfig};
use foundry_common::compile::ProjectCompiler;
use foundry_config::Config;
use serde_json::{json, Result};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::Command;
use zksync_contracts;

use super::utils_zksync;

// pub fn compile_zksync(config: &Config) -> eyre::Result<ProjectCompileOutput> {
pub fn compile_zksync(config: &Config, contract_path: &String) {
    let mut project = config.project().unwrap();
    let root_path = project.paths.root.display();
    let artifacts_path = project.paths.artifacts.display();
    // let zkout_path = &format!("{}{}", root_path, "/zksolc");
    let zkout_path = &format!("{}{}", artifacts_path, "/zksolc");
    let zk_abi_path = &format!("{}{}", zkout_path, "/abis");
    let zk_json_path = &format!("{}{}", zkout_path, "/standard_json");
    let build_path = &format!("{}{}", root_path, "/build");

    match fs::create_dir_all(std::path::Path::new(zkout_path)) {
        Ok(success) => println!("{:#?}, create zkout folder success", success),
        Err(error) => panic!("problem creating zkout folder: {:#?}", error),
    };

    match fs::create_dir_all(std::path::Path::new(zk_abi_path)) {
        Ok(success) => println!("{:#?}, create abi folder success", success),
        Err(error) => panic!("problem creating abi folder: {:#?}", error),
    };

    match fs::create_dir_all(std::path::Path::new(zk_json_path)) {
        Ok(success) => println!("{:#?}, create json folder success", success),
        Err(error) => panic!("problem creating json folder: {:#?}", error),
    };

    let zk_json_path = &format!("{}{}", zk_json_path, "/ctrcts.json");

    //check for compiler
    let zksolc_path = &format!("{}{}", zkout_path, "/zksolc-linux-amd64-musl-v1.3.7");
    let b = std::path::Path::new(zksolc_path).exists();

    if !b {
        utils_zksync::download_zksolc_compiler(zksolc_path, zkout_path);
    }

    println!("{:#?} <----- contract_path", contract_path);
    // let contract_path = &format!("{}{}", project.paths.sources.display(), "/Counter.sol");
    // let contract_path = &format!("{}{}", project.paths.sources.display(), "/Greeter.sol");
    // let contract_path = &format!("{}{}", root_path, "/src/AAExample.sol");
    let contract_full_path = &format!("{}/{}", root_path, contract_path);

    // let output_settings = OutputSelection::complete_output_selection();
    let output_settings = OutputSelection::default();
    let mut file_output_selection: FileOutputSelection = BTreeMap::default();
    file_output_selection.insert(
        "*".to_string(),
        vec![
            "abi".to_string(),
            // "metadata".to_string(),
            "evm.deployedBytecode".to_string(),
            "evm.methodIdentifiers".to_string(),
        ],
    );
    file_output_selection.insert(
        "".to_string(),
        vec![
            "ast".to_string(),
            // "metadata".to_string(),
            // "evm.deployedBytecode".to_string(),
            // "evm.methodIdentifiers".to_string(),
        ],
    );

    // let zk_compiler = ProjectCompiler::new(true, true);
    // let sources = project.sources().unwrap();
    let mut solc_config = SolcConfig::builder().build();
    solc_config.settings.output_selection = output_settings.clone();

    let solc_1 = Solc::new(zksolc_path).args([
        // contract_path,
        // "--abi",
        // "--bin",
        // "--hashes",
        "--optimize",
        "--standard-json",
        // "--combined-json",
        // "abi",
        // "abi,bin",
        // "--output-dir",
        // zk_abi_path,
        // "--overwrite",
    ]);
    solc_config.settings.output_selection.0.insert("*".to_string(), file_output_selection.clone());

    project.solc = solc_1.clone();
    project.solc_config = solc_config.clone();

    // println!("{:#?} zk_compiler", zk_compiler);
    // println!("{:#?} output_sel", solc_config.settings.output_selection);
    println!("{:#?} project solc", project.solc);
    println!("{:#?} project.solc_config", project.solc_config);
    // let compile_out = zk_compiler.compile(&project);
    // println!("{:#?} compile_out", compile_out);

    // let mut standard_json = project.standard_json_input(contract_full_path).unwrap();

    // standard_json.settings.output_selection = OutputSelection::default();
    // standard_json
    //     .settings
    //     .output_selection
    //     .0
    //     .insert("*".to_string(), file_output_selection.clone());
    // standard_json.settings.output_selection = output_settings;

    // let mut standard_json = project.standard_json_input(contract_full_path).unwrap();

    // println!("{:#?} standard_json", standard_json.settings);
    // let stdjson = serde_json::to_value(&standard_json).unwrap();

    // let mut file = match File::create("json.json") {
    //     Err(why) => panic!("couldn't create : {}", why),
    //     Ok(file) => file,
    // };

    // Save the JSON structure into the other file.
    // std::fs::write(zk_json_path, serde_json::to_string_pretty(&stdjson).unwrap()).unwrap();

    let compile_json = project.compile().unwrap();
    let artifacts = compile_json.clone().into_artifacts();
    for (id, artifact) in artifacts {
        let name = id.name;
        let abi = artifact.clone().abi.unwrap();
        // println!("{:#?} abi", abi);
        println!("CONTRACT: {:#?}", name);
        println!("ARTIFACT: {:#?}, artifact", artifact);
    }
    // println!("{:#?} compile_json", compile_json);

    let contracts = compile_json.clone().output();
    let write_build = contracts.write_build_infos(build_path).unwrap();
    println!("{:#?} write_build", write_build);

    // let bcodes = compile_json.into_contract_bytecodes();
    // for () in bcodes {}

    // let path = &format!("{}/{}", zkout_path, "compiled.json");
    // let path = Path::new(path);
    // let display = path.display();

    // let mut file = match File::create(path) {
    //     Err(why) => panic!("couldn't create {}: {}", display, why),
    //     Ok(file) => file,
    // };

    // file.write_all(&compile_json).unwrap();

    //-------------------------------------------//
    // THIS is all working for combined-json output
    //compile using combined-json flag
    // let output = Command::new(zksolc_path)
    //     .args([
    //         // contract_full_path,
    //         // "--abi",
    //         // "--bin",
    //         // "--hashes",
    //         // "--optimize",
    //         "--standard-json",
    //         // zk_json_path,
    //         // "json.json",
    //         // "--combined-json",
    //         // "bin",
    //         // "abi,bin,hashes,bin-runtime",
    //         // "--output-dir",
    //         // zkout_path,
    //         // "--overwrite",
    //         project.paths.sources.to_str().unwrap(),
    //     ])
    //     // .stdin(Stdio::piped())
    //     .output()
    //     .expect("failed to execute process");

    // let path = &format!("{}/{}", zkout_path, "artifacts.json");
    // let path = Path::new(path);
    // let display = path.display();

    // let mut file = match File::create(path) {
    //     Err(why) => panic!("couldn't create {}: {}", display, why),
    //     Ok(file) => file,
    // };

    // file.write_all(&output.stdout).unwrap();

    // ----------------------------------------------//

    // println!("{:#?} output", &output);

    // // let mut file = match File::create("foo.txt") {
    // //     Err(why) => panic!("couldn't create {}: {}", display, why),
    // //     Ok(file) => file,
    // // };

    // file.write_all(&output.stdout).unwrap();

    println!("{:#?} project", &project);
    // println!("{:#?} config", &config);

    // return compile_out;
}

fn get_artifact_output() -> Result<()> {
    // File hosts must exist in current path before this produces output
    if let Ok(lines) = read_lines(Path::new("foo.txt")) {
        // Consumes the iterator, returns an (Optional) String
        for line in lines {
            if let Ok(ip) = line {
                let value = json!(ip);
                let v: serde_json::Value = serde_json::from_str(&ip)?;
                // match &v {
                //     Ok(success) => println!("{:#?}, create_folder success", success),
                //     Err(error) => panic!("problem creating folder: {:#?}", error),
                // };

                println!("{}, value", v);
                println!("{}", ip);
            }
        }
    }
    Ok(())
}

// The output is wrapped in a Result to allow matching on errors
// Returns an Iterator to the Reader of the lines of the file.
fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}
