use ethers::prelude::{ContractFactory, Http, Provider, Wallet};
use ethers::solc::Project;
use ethers::types::{Address, Bytes};
use foundry_config::Config;
use serde_json;
use sha2::{Digest, Sha256};
use std::convert::TryFrom;
use std::env;
use std::fs;
use std::io::{Read, Result};
use zksync::operations::DeployContractBuilder;
use zksync::types::H256;
use zksync::zksync_eth_signer::{EthereumSigner, PrivateKeySigner};
use zksync::zksync_types::L2ChainId;
use zksync::{ethereum, EthereumProvider};
use zksync::{signer, wallet};

pub async fn deploy_zksync(config: &Config, project: &Project) -> Result<()> {
    // let rpc_url = config.get_rpc_url().unwrap().ok();
    // println!("{:#?}, rpc_url ---->>>", rpc_url);
    // println!("{:#?}, config ---->>>", config.rpc_endpoints.endpoints["goerli"]);

    let private_key = H256::from([5; 32]);
    let eth_signer = PrivateKeySigner::new(private_key);
    // println!("{:#?}, eth_signer ---->>>", eth_signer.get_address());
    let signer_addy = eth_signer.get_address().await.unwrap();
    // get signer
    let _signer = signer::Signer::new(eth_signer, signer_addy, L2ChainId(280));

    // // create a wallet
    let wallet = wallet::Wallet::with_http_client(
        "https://goerli.infura.io/v3/332aa765e52d4f219b8408485be193c1",
        _signer,
    )
    .unwrap();

    // Build Deployer
    let deployer_builder = DeployContractBuilder::new(&wallet);
    // println!("{:#?}, deployer_builder ---->>>", deployer_builder);

    //get abi and bytecode
    let output_path: &str = &format!("{}{}", project.paths.root.display(), "/zksolc/combined.json");
    let data = fs::read_to_string(output_path).expect("Unable to read file");
    //convert to json
    let res: serde_json::Value = serde_json::from_str(&data).expect("Unable to parse");
    // println!("{:#?}, combined.json ---->>>", res["contracts"]);
    // let contract = &res["contracts"]["src/Greeter.sol:Greeter"];
    // let contract_json = serde_json::from_str::<ethers::abi::JsonAbi>(contract).unwrap();
    // println!("{:#?}, contract_json ---->>>", contract_json.get("bin") );
    // println!("{:#?}, project ---->>>", project);

    //get abi
    let abi_path = &format!("{}{}", project.paths.artifacts.display(), "/Greeter.sol/Greeter.json");
    // println!("{:#?}, abi_path ---->>>", abi_path);
    let abi_data = fs::read_to_string(abi_path).expect("Unable to read file");
    // println!("{:#?}, abi_data ---->>>", abi_data);
    let res1: serde_json::Value = serde_json::from_str(&abi_data).expect("Unable to parse");
    let rawabi = res1["abi"].clone();
    // println!("{:#?}, res1 ---->>>", res1["abi"]);
    let abi: ethers::abi::Abi = serde_json::from_value(rawabi.clone())?;
    // println!("{:#?}, rawabi ---->>>", rawabi[0]);

    // let abi_string = rawabi[0].as_object();
    // let abi_string = match abi_string {
    //     Some(value) => value,
    //     None => panic!("error abi_string"),
    // };
    // println!("{:#?}, abi_string ---->>>", abi_string);

    let bytecode: Bytes =
        serde_json::from_value(res["contracts"]["src/Greeter.sol:Greeter"]["bin"].clone()).unwrap();
    // println!("{:#?}, bytecode ---->>>", bytecode);

    let bytecode_array = &bytecode.bytes();
    // println!("{:#?}, bytecodeArray ---->>>", bytecode_array);

    // create a Sha256 object
    let mut hasher = Sha256::new();

    // write input message
    // hasher.update(bytecodeArray);
    hasher.update(&bytecode);
    println!("{:#?}, hasher ---->>>", hasher);

    // read hash digest and consume hasher
    let bytecode_hash = hasher.finalize();
    // println!("{:#?}, bytecodeHash ---->>>", bytecode_hash);

    let bytecode_hash_bytes = bytecode_hash.bytes();
    // println!("{:#?}, bytecode_hash_bytes ---->>>", bytecode_hash_bytes);
    // connect to the network
    // let client = Provider::<Http>::try_from("https://zksync2-testnet.zksync.dev").unwrap();
    let client =
        Provider::<Http>::try_from("https://goerli.infura.io/v3/332aa765e52d4f219b8408485be193c1")
            .unwrap();

    let client = std::sync::Arc::new(client);
    println!("{:#?}, client url---->>>", client.url());
    // println!("{:#?}, project ---->>>", project);

    // create a factory which will be used to deploy instances of the contract
    let factory = ContractFactory::new(abi, bytecode, client);
    // println!("{:#?}, factory ---->>>", factory);

    Ok(())
}
