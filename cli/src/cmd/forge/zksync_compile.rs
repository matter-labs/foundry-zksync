use super::utils_zksync;
use ethers::solc::artifacts::output_selection::FileOutputSelection;
use ethers::solc::Graph;
use foundry_config::Config;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::process::{Command, Stdio};

pub struct CombilerArgs {
    pub contract_path: &String,
    pub force_evmla: bool,
    pub system_mode: bool,
    pub llvm_debug_logging: bool,
    pub llvm_ir: bool,
    pub llvm_verify_each: bool,
    pub asm: bool,
    pub bin: bool,
    pub overwrite: bool,
    pub standard_json: bool,
    pub yul: bool,
}

pub fn compile_zksync(config: &Config, args: CombilerArgs) {
    // let zk_account = utils_zksync::get_test_account();
    // println!("{:#?}, zk_account", zk_account);
    // utils_zksync::check_testing();

    // get compiler filename
    let compiler_filename = utils_zksync::get_zksolc_filename();

    let mut project = config.project().unwrap();
    project.auto_detect = false;
    let contract_full_path = project.paths.sources.join(contract_path);
    let zkout_path = project.paths.artifacts.join("zksolc");
    let build_path = zkout_path.join(contract_path);

    match fs::create_dir_all(&build_path) {
        Ok(()) => println!(" create build_path folder success"),
        Err(error) => panic!("problem creating build_path folder: {:#?}", error),
    };

    //check for compiler
    let zksolc_path = zkout_path.join(&compiler_filename);
    if !zksolc_path.exists() {
        utils_zksync::download_zksolc_compiler(zksolc_path.clone(), zkout_path, &compiler_filename);
    }

    // Get output selection
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

    let standard_json = project.standard_json_input(&contract_full_path).unwrap();
    // println!("{:#?} standard_json", standard_json);

    // Save the JSON input to build folder.
    let stdjson = serde_json::to_value(&standard_json).unwrap();
    let path = build_path.join("json_input.json");
    match File::create(&path) {
        Err(why) => panic!("couldn't create : {}", why),
        Ok(file) => file,
    };
    std::fs::write(path, serde_json::to_string_pretty(&stdjson).unwrap()).unwrap();

    //detect solc
    let graph = Graph::resolve_sources(&project.paths, project.sources().expect("REASON")).unwrap();
    let (versions, _edges) = graph.into_sources_by_version(project.offline).unwrap();
    let solc_version = versions.get(&project).unwrap();
    let solc_v_path = Some(&solc_version.first_key_value().unwrap().0.solc);
    println!("{:#?}, solc_v", solc_v_path);

    //build output paths
    let path = build_path.join("artifacts.json");
    let display = path.display();
    let mut file = match File::create(&path) {
        Err(e) => panic!("couldn't create {}: {}", display, e),
        Ok(file) => file,
    };

    //build args
    let mut comp_args: Vec<String> = vec![contract_full_path.to_str().unwrap().to_string()];

    comp_args.push("--standard-json".to_string());

    if let Some(_path) = solc_v_path {
        comp_args.push("--solc".to_string());
        comp_args.push(solc_v_path.unwrap().to_str().unwrap().to_string());
    }

    //TODO: also check --use build command for changing solc version

    if args.force_evmla {
        comp_args.push("--force-evmla".to_string());
    }
    if args.system_mode {
        comp_args.push("--system-mode".to_string());
    }
    if args.llvm_debug_logging {
        comp_args.push("--llvm-debug-logging".to_string());
    }
    if args.llvm_ir {
        comp_args.push("--llvm-ir".to_string());
    }
    if ars.llvm_verify_each {
        comp_args.push("--llvm-verify-each".to_string());
    }
    if args.asm {
        comp_args.push("--asm".to_string());
    }
    if args.bin {
        comp_args.push("--bin".to_string());
    }
    if args.overwrite {
        comp_args.push("--overwrite".to_string());
    }
    if args.standard_json {
        comp_args.push("--standard-json".to_string());
    }
    if args.yul {
        comp_args.push("--yul".to_string());
    }

    let mut cmd = Command::new(zksolc_path);
    let mut child = cmd
        .args(comp_args)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn();
    let stdin = child.as_mut().unwrap().stdin.take().expect("Stdin exists.");
    serde_json::to_writer(stdin, &standard_json).unwrap();
    let output = child.unwrap().wait_with_output();

    file.write_all(&output.unwrap().stdout).unwrap();

    // println!("{:#?} output", &output);
    // ----------------------------------------------//
}
