use downloader::{Download, Downloader};
use ethers::solc::{ArtifactId, ConfigurableContractArtifact, Project, Solc};
use foundry_common::compile::{self, ProjectCompiler, SkipBuildFilter};
use foundry_config::Config;
use std::fs::set_permissions;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::prelude::PermissionsExt;
use std::process::Command;

pub fn compile_zksync(config: &Config) {
    let mut project = config.project().unwrap();

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

    // let contract_path = &format!("{}{}", project.paths.sources.display(), "/Counter.sol");
    let contract_path = &format!("{}{}", project.paths.sources.display(), "/Greeter.sol");

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

    //create new Solc object
    // let zk_compiler = Solc::new(zksolc_path).args([
    //     contract_path,
    //     "--abi",
    //     "--bin",
    //     // "--combined-json",
    //     // "abi,hashes,bin",
    //     // "--output-dir",
    //     // zkout_path,
    //     // "--overwrite",
    // ]);
    // println!("{:#?} zk_compiler", &zk_compiler);
    // let compiler = ProjectCompiler::with_filter(
    //     false,
    //     false,
    //     vec![SkipBuildFilter::Tests, SkipBuildFilter::Scripts],
    // );
    // // configure compiler object
    // project.solc = zk_compiler;
    // println!("{:#?} project", &project);
    // let compile_result = compiler.compile(&project).unwrap();
    // println!("{:#?} compile_result", &compile_result.compiled_artifacts());
    // let arts = project.compile().unwrap();
    // println!("{:#?} arts", arts);
    // let mut buffer = File::create("foo.txt").unwrap();

    // buffer.write_all(&compile_result).unwrap();

    //

    let output = Command::new(zksolc_path)
        .args([
            contract_path,
            "--abi",
            "--bin",
            "--hashes",
            "--optimize",
            "--combined-json",
            "abi,bin,bin-runtime",
            "--output-dir",
            zkout_path.trim(),
            "--overwrite",
        ])
        .output()
        .expect("failed to execute process");

    let mut buffer = File::create("foo.json").unwrap();

    println!("{:#?} output", &output);
    buffer.write_all(&output.stdout).unwrap();
    // println!("{:#?} project", &project);
    // println!("{:#?} config", &config);
}
