use ethers::abi::{encode, Token};
use ethers::prelude::{ContractFactory, Http, Project, Provider, Wallet};
use ethers::types::{Address, Bytes};
use foundry_config::Config;
use serde_json;
use std::convert::TryFrom;
use std::io::{Read, Result};
use std::{env, fs};
use zksync;
use zksync::operations::DeployContractBuilder;
use zksync::types::H256;
use zksync::zksync_eth_signer::{EthereumSigner, PrivateKeySigner};
use zksync::zksync_types::{L2ChainId, PackedEthSignature};
use zksync::{ethereum, signer, wallet, EthereumProvider};

pub async fn deploy_zksync(
    config: &Config,
    project: &Project,
    constructor_params: Vec<Token>,
) -> Result<()> {
    // println!("{:#?}, project ---->>>", project);
    // println!("{:#?}, config ---->>>", config);
    // println!("{:#?}, constructor_params ---->>>", constructor_params);

    //test env vars
    let path = env::current_dir()?;
    println!("The current directory is {}", path.display());

    //get abi and bytecode
    let output_path: &str = &format!("{}{}", project.paths.root.display(), "/zksolc/combined.json");
    let data = fs::read_to_string(output_path).expect("Unable to read file");
    //convert to json
    let res: serde_json::Value = serde_json::from_str(&data).expect("Unable to parse");
    // println!("{:#?}, combined.json ---->>>", res["contracts"]);
    let contract = &res["contracts"]["src/Greeter.sol:Greeter"];
    // let contract = &res["contracts"]["src/Counter.sol:Counter"];

    // let contract_json = serde_json::from_str::<ethers::abi::JsonAbi>(contract).unwrap();
    // println!("{:#?}, contract_json ---->>>", contract_json.get("bin") );

    // //get abi
    // let abi_path = &format!("{}{}", project.paths.artifacts.display(), "/Greeter.sol/Greeter.json");
    // // println!("{:#?}, abi_path ---->>>", abi_path);
    // let abi_data = fs::read_to_string(abi_path).expect("Unable to read file");
    // // println!("{:#?}, abi_data ---->>>", abi_data);
    // let res1: serde_json::Value = serde_json::from_str(&abi_data).expect("Unable to parse");
    // let rawabi = res1["abi"].clone();
    // // println!("{:#?}, res1 ---->>>", res1["abi"]);
    // let abi: ethers::abi::Abi = serde_json::from_value(rawabi.clone())?;
    // println!("{:#?}, rawabi ---->>>", rawabi[0]);

    // let abi_string = rawabi[0].as_object();
    // let abi_string = match abi_string {
    //     Some(value) => value,
    //     None => panic!("error abi_string"),
    // };
    // println!("{:#?}, abi_string ---->>>", abi_string);

    //------------------------------------------//

    let pk = "d5b54c3da4bd2722bb9dd3df5aa86e71b8db43560be88b1a271feb4df3268b54";
    let p_key = decode_hex(pk).unwrap();
    let priv_key: &[u8] = &p_key;
    let private_key = H256::from_slice(priv_key);

    let eth_signer = PrivateKeySigner::new(private_key);
    let signer_addy = PackedEthSignature::address_from_private_key(&private_key)
        .expect("Can't get an address from the private key");
    let _signer = signer::Signer::new(eth_signer, signer_addy, L2ChainId(280));
    // println!("{:#?}, _signer ---->>>", _signer);

    let deployer_builder;
    let wallet =
        wallet::Wallet::with_http_client("https://zksync2-testnet.zksync.dev:443", _signer);
    match &wallet {
        Ok(w) => {
            // Build Deployer //
            deployer_builder = w.start_deploy_contract();
            println!("{:#?}, deployer_builder ---->>>", w);
        }
        Err(e) => panic!("error wallet: {e:?}"),
    };
    // let bytecode: Bytes =
    //     serde_json::from_value(res["contracts"]["src/Counter.sol:Counter"]["bin"].clone()).unwrap();
    let bytecode: Bytes =
        serde_json::from_value(res["contracts"]["src/Greeter.sol:Greeter"]["bin"].clone()).unwrap();
    let bytecode_v = bytecode.to_vec();
    let bytecode_array: &[u8] = &bytecode_v;

    //validate bytecode
    match zksync_utils::bytecode::validate_bytecode(bytecode_array) {
        Ok(_success) => println!("bytecode isValid"),
        Err(e) => println!("{e:#?} bytecode not valid"),
    }

    //get bytecode hash
    // let bytecode_hash = zksync_utils::bytecode::hash_bytecode(bytecode_array);
    // println!("{:#?}, bytecode_hash ---->>>", bytecode_hash);

    let mut construct_args = Vec::<u8>::new();
    let param_array = constructor_params.as_slice();
    let encoded_args = encode(param_array);

    println!("{:#?}, encoded_args ---->>>", encoded_args);

    // for arg in constructor_params {
    //     println!("{:#?}, arg ---->>>", arg);
    //     if let Some(value) = arg.into_string() {
    //         println!("{:#?}, value ---->>>", value.as_bytes());
    //         // construct_args = value.clone();
    //     }
    // }
    // println!("{:#?}, construct_args ---->>>", construct_args);

    //factory deps
    let factory_deps = vec![bytecode_v.clone()];

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
        Ok(dep) => println!("{dep:?}, deploy success"),
        Err(e) => println!("{:#?}, error", e),
    }
    println!("<---- IGNORE ERRORS BELOW THIS LINE---->>>");

    // connect to the network
    // let zk_client = Provider::<Http>::try_from("https://zksync2-testnet.zksync.dev").unwrap();
    let eth_client =
        Provider::<Http>::try_from("https://goerli.infura.io/v3/332aa765e52d4f219b8408485be193c1")
            .unwrap();

    let client = std::sync::Arc::new(eth_client);
    // println!("{:#?}, client url---->>>", client.url());

    // create a factory which will be used to deploy instances of the contract
    // let factory = ContractFactory::new(abi, bytecode_counter, client);
    // println!("{:#?}, factory ---->>>", factory);

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
