use foundry_config::Config;
use std::fs::{self, File};
use std::io::Write;
use std::process::Command;
use zksync_contracts;

use super::utils_zksync;

pub fn compile_zksync(config: &Config) {
    let project = config.project().unwrap();
    let root_path = project.paths.root.display();
    let zkout_path = &format!("{}{}", root_path, "/zksolc");
    let create_folder = fs::create_dir_all(std::path::Path::new(zkout_path));
    match create_folder {
        Ok(success) => println!("{:#?}, create_folder success", success),
        Err(error) => panic!("problem creating folder: {:#?}", error),
    };

    //check for compiler
    let zksolc_path = &format!("{}{}", zkout_path, "/zksolc-linux-amd64-musl-v1.3.5");
    let b = std::path::Path::new(zksolc_path).exists();

    if !b {
        utils_zksync::download_zksolc_compiler(zksolc_path, zkout_path);
    }

    // let contract_path = &format!("{}{}", project.paths.sources.display(), "/Counter.sol");
    // let contract_path = &format!("{}{}", project.paths.sources.display(), "/Greeter.sol");
    let contract_path = &format!("{}{}", root_path, "/src/AAExample.sol");

    let output = Command::new(zksolc_path)
        .args([
            contract_path,
            "--abi",
            "--bin",
            // "--hashes",
            // "--optimize",
            // "--combined-json",
            // "abi",
            // "abi,bin,bin-runtime",
            // "--output-dir",
            // zkout_path.trim(),
            // "--overwrite",
        ])
        // .stdin(cfg)
        .output()
        .expect("failed to execute process");

    // let output = Command::new(zksolc_path)
    //     .args([contract_path, "--bin"])
    //     .output()
    //     .expect("failed to execute process");

    let mut buffer = File::create("foo.txt").unwrap();

    println!("{:#?} output", &output);
    buffer.write_all(&output.stdout).unwrap();

    // println!("{:#?} project", &project);
    // println!("{:#?} config", &config);
}
