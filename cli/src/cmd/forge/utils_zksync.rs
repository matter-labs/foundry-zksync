use downloader::{Download, Downloader};
use ethers::abi::ethabi;
use serde_json;
use std::fs::set_permissions;
use std::os::unix::prelude::PermissionsExt;
use std::str::FromStr;

pub fn load_contract(raw_abi_string: &str) -> ethabi::Contract {
    let abi_string = serde_json::Value::from_str(raw_abi_string)
        .expect("Malformed contract abi file")
        .get("abi")
        .expect("Malformed contract abi file")
        .to_string();
    ethabi::Contract::load(abi_string.as_bytes()).unwrap()
}

pub fn download_zksolc_compiler(zksolc_path: &String, zkout_path: &String) {
    let download: Download = Download::new("https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.7");
    //get downloader builder
    let mut builder = Downloader::builder();
    //assign download folder
    builder.download_folder(std::path::Path::new(zkout_path));
    //build downloader
    let mut d_loader = builder.build().unwrap();

    //download compiler
    let d_load = d_loader.download(&[download]);
    match d_load {
        Ok(success) => println!("{:#?},  d_load success", success),
        Err(error) => panic!("problem d_load: {:#?}", error),
    };

    let perm = set_permissions(std::path::Path::new(zksolc_path), PermissionsExt::from_mode(0o755));
    match perm {
        Ok(success) => println!("{:#?}, set permissions success", success),
        Err(error) => panic!("problem setting permissions: {:#?}", error),
    };
}
