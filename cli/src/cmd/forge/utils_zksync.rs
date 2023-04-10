use downloader::{Download, Downloader};
use ethers::abi::ethabi;
use serde_json;
use std::env;
use std::fs::set_permissions;
use std::os::unix::prelude::PermissionsExt;
use std::str::FromStr;
use std::time::Duration;
use tempfile::TempDir;
use vm::vm_with_bootloader::BlockContext;
use zksync_storage::db;
use zksync_types::{FAIR_L2_GAS_PRICE, H160};

use zksync_test_account::ZkSyncAccount;

use zk_evm;

pub fn load_contract(raw_abi_string: &str) -> ethabi::Contract {
    let abi_string = serde_json::Value::from_str(raw_abi_string)
        .expect("Malformed contract abi file")
        .get("abi")
        .expect("Malformed contract abi file")
        .to_string();
    ethabi::Contract::load(abi_string.as_bytes()).unwrap()
}

pub fn get_zksolc_filename() -> String {
    //get compiler filename
    let mut compiler_filename = String::from("/zksolc-");
    let mut extension = String::new();
    let mut toolchain = String::new();
    let mut architecture = String::new();
    let key = "OS";
    match env::var(key) {
        Ok(val) => {
            compiler_filename.push_str(&val);
            compiler_filename.push('-');
            println!("{key}: {val:?}");
            if val.eq("linux") {
                toolchain.push_str("musl-");
                architecture.push_str("amd64-");
            }
            if val.eq("macosx") {
                match env::var("ARCH") {
                    Ok(val) => {
                        architecture.push_str(&val);
                        architecture.push('-');
                        println!("{key}: {val:?}");
                    }
                    Err(e) => println!("couldn't interpret {key}: {e}"),
                }
            }
            if val.eq("windows") {
                extension.push_str(".exe");
                toolchain.push_str("gnu");
                architecture.push_str("amd64");
            }
        }
        Err(e) => println!("couldn't interpret {key}: {e}"),
    }

    compiler_filename.push_str(&architecture);
    compiler_filename.push_str(&toolchain);

    let key = "ZKSOLC_COMPILER_VERSION";
    match env::var(key) {
        Ok(val) => {
            compiler_filename.push('v');
            compiler_filename.push_str(&val);
            println!("{key}: {val:?}");
        }
        Err(e) => println!("couldn't interpret {key}: {e}"),
    }
    println!("{:#?}, compiler_filename", compiler_filename);
    compiler_filename
}

pub fn download_zksolc_compiler(
    zksolc_path: &String,
    zkout_path: &String,
    compiler_filename: String,
) {
    let zksolc_path = &format!("{}{}", zkout_path, compiler_filename);

    //get download folder
    let parts: Vec<&str> = compiler_filename.split("-").collect();
    let mut download_folder = String::from(parts[1]);
    download_folder.push('-');
    download_folder.push_str(parts[2]);
    println!("{:#?} download_folder", download_folder);

    let download_url = &format!(
        "{}{}{}",
        "https://github.com/matter-labs/zksolc-bin/raw/main/", download_folder, compiler_filename
    );
    let download: Download = Download::new(download_url);
    //get downloader builder
    let mut builder = Downloader::builder();
    //assign download folder
    builder
        .download_folder(std::path::Path::new(zkout_path))
        .connect_timeout(Duration::from_secs(240));
    //build downloader
    let mut d_loader = builder.build().unwrap();

    //download compiler
    let d_load = d_loader.download(&[download]);
    match d_load {
        Ok(success) => println!("{:#?},  compiler download success", success),
        Err(error) => panic!("problem downloading compiler: {:#?}", error),
    };

    set_zksolc_permissions(zksolc_path);
}

fn set_zksolc_permissions(zksolc_path: &String) {
    let perm =
        set_permissions(std::path::Path::new(&zksolc_path), PermissionsExt::from_mode(0o755));
    match perm {
        Ok(success) => println!("{:#?}, set permissions success", success),
        Err(error) => panic!("problem setting permissions: {:#?}", error),
    };
}

pub fn get_test_account() -> ZkSyncAccount {
    ZkSyncAccount::rand()
}

pub fn get_def_block_context() -> BlockContext {
    BlockContext {
        block_number: 1u32,
        block_timestamp: 1000,
        l1_gas_price: 50_000_000_000, // 50 gwei
        fair_l2_gas_price: FAIR_L2_GAS_PRICE,
        operator_address: H160::zero(),
    }
}

// use zksync_core::api_server::execution_sandbox;
// use zksync_state::secondary_storage;

pub fn check_testing() {
    let context = get_def_block_context();
    println!("{:#?}, context", context);

    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let db = db::RocksDB::new(db::Database::StateKeeper, temp_dir.as_ref(), false);
    println!("{:#?}, db", db);
    // let mut raw_storage = secondary_storage::SecondaryStateStorage::new(db);
    // println!("{:#?}, raw_storage", raw_storage);
    // vm::utils::insert_system_contracts(&mut raw_storage);

    let tools = zk_evm::testing::create_default_testing_tools();
    // println!("{:#?}, tools.decommittment_processor", tools.decommittment_processor);
    // println!("{:#?}, tools.event_sink", tools.event_sink);
    // println!("{:#?}, tools.storage", tools.storage);
    // println!("{:#?}, tools.memory", tools.memory);
    // println!("{:#?}, tools.witness_tracer", tools.witness_tracer);
    // println!("{:#?}, tools.precompiles_processor", tools.precompiles_processor);

    // let exec = execution_sandbox::execute_tx_eth_call(
    //     ConnectionPool::new(Some(50), true),

    // );
}
