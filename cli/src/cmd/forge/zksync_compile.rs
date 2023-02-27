use downloader::{Download, Downloader};
use ethers::solc::Project;
use foundry_config::Config;
use std::fs;
use std::fs::set_permissions;
use std::os::unix::prelude::PermissionsExt;
use std::process::Command;

pub fn compile_zksync(config: &Config, project: &Project) {
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

    let contract_path = &format!("{}{}", project.paths.sources.display(), "/Counter.sol");
    // let contract_path = &format!("{}{}", project.paths.sources.display(), "/Greeter.sol");

    if !b {
        let download: Download = Download::new("https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.5");
        //get downloader builder
        let mut builder = Downloader::builder();
        //assign download folder
        builder.download_folder(std::path::Path::new(&format!(
            "{}{}",
            project.paths.root.display(),
            "/zksolc"
        )));
        //build downloader
        let mut d_loader = builder.build().unwrap();

        //download compiler
        let d_load = d_loader.download(&[download]);
        match d_load {
            Ok(success) => println!("{:#?},  d_load success", success),
            Err(error) => panic!("problem d_load: {:#?}", error),
        };

        let perm =
            set_permissions(std::path::Path::new(zksolc_path), PermissionsExt::from_mode(0o755));
        match perm {
            Ok(success) => println!("{:#?}, set permissions success", success),
            Err(error) => panic!("problem setting permissions: {:#?}", error),
        };
    }

    let output = Command::new(zksolc_path)
        .args([
            contract_path,
            "--abi",
            "--combined-json",
            "abi,hashes,bin",
            "--output-dir",
            zkout_path,
            "--overwrite",
        ])
        .output()
        .expect("failed to execute process");

    println!("{:#?} output", &output);

    println!("{:#?} project", &project);
    println!("{:#?} config", &config);
}
