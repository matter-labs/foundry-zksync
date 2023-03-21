use ethers::abi::{encode, Token};
use ethers::prelude::Project;
use ethers::solc::info::ContractInfo;
use ethers::types::Bytes;
use foundry_config::Config;
use rustc_hex::ToHex;
use serde_json;
use std::fs;
use std::io::Result;
use zksync::types::H256;
use zksync::zksync_eth_signer::PrivateKeySigner;
use zksync::zksync_types::{L2ChainId, PackedEthSignature};
use zksync::{self, signer, wallet};
use zksync_types::{Address, CONTRACT_DEPLOYER_ADDRESS};

pub async fn deploy_zksync(
    _config: &Config,
    project: &Project,
    constructor_params: Vec<Token>,
    contract_info: ContractInfo,
    priv_key: String,
    rpc_url: String,
    contract_path: String,
    chain_id: u16,
) -> Result<()> {
    let contract_name = contract_info.name;
    let mut filename = contract_path.split('/');
    let splits = filename.clone().count();
    let file = filename.nth(splits - 1).unwrap().split(":").nth(0).unwrap();

    //get standard json output
    let output_path: &str =
        &format!("{}/zksolc/{}/artifacts.json", project.paths.artifacts.display(), file);
    let data = fs::read_to_string(output_path).expect("Unable to read file");
    //convert to json Value
    let res: serde_json::Value = serde_json::from_str(&data).expect("Unable to parse");
    // get bytecode
    let bytecode: Bytes = serde_json::from_value(
        res["contracts"][&contract_path][contract_name]["evm"]["bytecode"]["object"].clone(),
    )
    .unwrap();
    let bytecode_v = bytecode.to_vec();

    //---------------------------------//
    // //validate bytecode
    // let bytecode_array: &[u8] = &bytecode_v;
    // match zksync_utils::bytecode::validate_bytecode(bytecode_array) {
    //     Ok(_success) => println!("bytecode isValid"),
    //     Err(e) => println!("{e:#?} bytecode not valid"),
    // }
    // //get bytecode hash
    // let bytecode_hash = zksync_utils::bytecode::hash_bytecode(bytecode_array);
    //---------------------------------//

    // get signer
    let private_key = H256::from_slice(&decode_hex(&priv_key).unwrap());
    let eth_signer = PrivateKeySigner::new(private_key);
    let signer_addy = PackedEthSignature::address_from_private_key(&private_key)
        .expect("Can't get an address from the private key");
    let _signer = signer::Signer::new(eth_signer, signer_addy, L2ChainId(chain_id));

    // encode constructor args
    let encoded_args = encode(constructor_params.as_slice());
    let _hex_args = &encoded_args.to_hex::<String>();

    //factory deps
    let factory_deps = vec![bytecode_v.clone()];

    let deployer_builder;
    let wallet = wallet::Wallet::with_http_client(&rpc_url, _signer);
    match &wallet {
        Ok(w) => {
            deployer_builder = w.start_deploy_contract();
        }
        Err(e) => panic!("error wallet: {e:?}"),
    };

    let deployer = deployer_builder
        .bytecode(bytecode_v)
        .factory_deps(factory_deps)
        .constructor_calldata(encoded_args);

    let est_gas = deployer.estimate_fee(None).await;
    match est_gas {
        Ok(fee) => println!("{:#?}, est fee success", fee),
        Err(c) => println!("{:#?}, error", c),
    }

    let tx = deployer.send().await;
    match tx {
        Ok(dep) => {
            let rcpt = dep.wait_for_commit().await;
            println!("{dep:#?}, deploy success");
            let logs = rcpt.unwrap().logs;
            for log in logs {
                if log.address == CONTRACT_DEPLOYER_ADDRESS {
                    let deployed_address = log.topics.get(3).unwrap();
                    let deployed_address = Address::from(deployed_address.clone());
                    println!("{:#?}, <---- Deployed contract address:", deployed_address);
                }
            }
        }
        Err(e) => println!("{:#?}, error", e),
    }

    // println!("<---- IGNORE ERRORS BELOW THIS LINE---->>>");

    Ok(())
}

use std::{fmt::Write, num::ParseIntError};

pub fn decode_hex(s: &str) -> std::result::Result<Vec<u8>, ParseIntError> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16)).collect()
}

pub fn encode_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}
