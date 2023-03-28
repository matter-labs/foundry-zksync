use super::utils_zksync;
use ethers::solc::artifacts::output_selection::FileOutputSelection;
use ethers::solc::Graph;
use foundry_config::Config;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn compile_zksync(config: &Config, contract_path: &String) {
    let mut project = config.project().unwrap();
    project.auto_detect = false;
    let contract_full_path = &format!("{}/{}", project.paths.sources.display(), contract_path);
    let zkout_path = &format!("{}{}", project.paths.artifacts.display(), "/zksolc");
    let build_path = &format!("{}/{}", zkout_path, contract_path);

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

    let graph = Graph::resolve_sources(&project.paths, project.sources().expect("REASON")).unwrap();
    let (versions, edges) = graph.into_sources_by_version(project.offline).unwrap();

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

    let standard_json = project.standard_json_input(contract_full_path).unwrap();
    println!("{:#?} standard_json", standard_json);

    // Save the JSON input to build folder.
    let stdjson = serde_json::to_value(&standard_json).unwrap();
    let path = &format!("{}/{}", build_path, "json_input.json");
    let path = Path::new(path);
    match File::create(path) {
        Err(why) => panic!("couldn't create : {}", why),
        Ok(file) => file,
    };
    std::fs::write(path, serde_json::to_string_pretty(&stdjson).unwrap()).unwrap();

    //detect solc
    let solc_version = versions.get(&project).unwrap();
    let solc_v_path = Some(&solc_version.first_key_value().unwrap().0.solc);
    println!("{:#?}, solc_v", solc_v_path);

    //build output paths
    let path = &format!("{}/{}", build_path, "artifacts.json");
    let path = Path::new(path);
    let display = path.display();
    let mut file = match File::create(path) {
        Err(e) => panic!("couldn't create {}: {}", display, e),
        Ok(file) => file,
    };

    if let Some(path) = solc_v_path {
        // compile project
        let mut cmd = Command::new(zksolc_path);
        let mut child = cmd
            .args([
                contract_full_path,
                "--standard-json",
                "--solc",
                solc_v_path.unwrap().to_str().unwrap(),
            ])
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn();
        let stdin = child.as_mut().unwrap().stdin.take().expect("Stdin exists.");
        serde_json::to_writer(stdin, &standard_json).unwrap();
        let output = child.unwrap().wait_with_output();

        file.write_all(&output.unwrap().stdout).unwrap();
    } else {
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
        file.write_all(&output.unwrap().stdout).unwrap();
    }

    // println!("{:#?} output", &output);
    // ----------------------------------------------//
}
