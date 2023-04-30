//! Create command

use crate::{
    cmd::{forge::build::CoreBuildArgs, read_constructor_args_file},
    opts::{EthereumOpts, TransactionOpts},
};
use clap::{Parser, ValueHint};
use ethers::{
    abi::{encode, Abi, Constructor, Token},
    solc::{info::ContractInfo, Project},
    types::Bytes,
};
use foundry_common::abi::parse_tokens;
use foundry_config::Chain;

use serde_json::Value;

use std::{
    fs::{self},
    path::PathBuf,
};

use zksync::types::H256;
use zksync::zksync_eth_signer::PrivateKeySigner;
use zksync::zksync_types::{L2ChainId, PackedEthSignature};
use zksync::{self, signer::Signer, wallet};

/// CLI arguments for `forge zk-create`.
#[derive(Debug, Clone, Parser)]
#[clap(next_help_heading = "ZkCreate options", about = None)]
pub struct ZkCreateArgs {
    #[clap(
        help = "The contract identifier in the form `<path>:<contractname>`.",
        value_name = "CONTRACT"
    )]
    contract: ContractInfo,

    #[clap(
        long,
        num_args(1..),
        help = "The constructor arguments.",
        name = "constructor_args",
        conflicts_with = "constructor_args_path",
        value_name = "ARGS"
    )]
    constructor_args: Vec<String>,

    #[clap(
        long,
        help = "The path to a file containing the constructor arguments.",
        value_hint = ValueHint::FilePath,
        name = "constructor_args_path",
        conflicts_with = "constructor_args",
        value_name = "FILE"
    )]
    constructor_args_path: Option<PathBuf>,

    #[clap(
        long,
        num_args(1..),
        help_heading = "ZkSync Features",
        help = "The factory dependencies in the form `<path>:<contractname>`.",
        value_name = "FACTORY-DEPS"
    )]
    factory_deps: Option<Vec<ContractInfo>>,

    #[clap(flatten)]
    opts: CoreBuildArgs,

    #[clap(flatten)]
    tx: TransactionOpts,

    #[clap(flatten)]
    eth: EthereumOpts,
}

impl ZkCreateArgs {
    /// Executes the command to create a contract
    pub async fn run(self) -> eyre::Result<()> {
        // println!("{:#?}, ZkCreateArgs ---->>>", self);

        //get private key
        let private_key = match &self.eth.wallet.private_key {
            Some(pkey) => {
                let decoded = match decode_hex(pkey) {
                    Ok(val) => H256::from_slice(&val),
                    Err(e) => {
                        panic!("Error parsing private key {e}, make sure it is valid.")
                    }
                };
                decoded
            }
            None => {
                panic!("Private key was not provided. Try using --private-key flag");
            }
        };

        //verify rpc url has been populated
        if let None = &self.eth.rpc_url {
            panic!("RPC URL was not provided. Try using --rpc-url flag or environment variable 'ETH_RPC_URL= '");
        }

        //get chain
        let chain = match self.eth.chain {
            Some(chain) => chain,
            None => {
                panic!("Chain was not provided. Use --chain flag (ex. --chain 270 ) or environment variable 'CHAIN= ' (ex.'CHAIN=270')");
            }
        };

        // get project
        let mut project = self.opts.project()?;
        // set out folder path
        project.paths.artifacts = project.paths.root.join("zkout");

        // get  contract bytecode
        let bytecode = Self::get_bytecode_from_contract(&project, &self.contract)
            .unwrap_or_else(|e| panic!("Error getting bytecode from contract: {}", e));

        //check for additional factory deps
        let mut factory_deps = Vec::new();
        if let Some(fdep_contract_info) = self.factory_deps.clone() {
            factory_deps =
                self.get_factory_dependencies(&project, factory_deps, fdep_contract_info);
        }

        // get signer
        let signer = Self::get_signer(private_key, &chain);

        // get abi
        let abi = Self::get_abi_from_contract(&project, &self.contract)
            .unwrap_or_else(|e| panic!("Error gettting ABI from contract: {}", e));
        let contract: Abi = serde_json::from_value(abi)
            .unwrap_or_else(|e| panic!("Error converting json abi to Contract ABI: {}", e));

        // encode constructor args
        let encoded_args = encode(self.get_constructor_args(&contract).as_slice());

        let wallet = wallet::Wallet::with_http_client(&self.eth.rpc_url.unwrap(), signer);
        let deployer_builder = match &wallet {
            Ok(w) => w.start_deploy_contract(),
            Err(e) => panic!("error wallet: {e:?}"),
        };

        let deployer = deployer_builder
            .bytecode(bytecode.to_vec())
            .factory_deps(factory_deps)
            .constructor_calldata(encoded_args);

        // TODO: could be useful as a flag --estimate-gas
        // let est_gas = deployer.estimate_fee(None).await;
        // println!("{:#?}, est_gas", est_gas);

        println!("Deploying contract...");
        let tx = deployer.send().await;
        match tx {
            Ok(deploy) => {
                let rcpt = deploy
                    .wait_for_commit()
                    .await
                    .unwrap_or_else(|e| panic!("Transaction Error: {}", e));
                let deployed_address = rcpt
                    .contract_address
                    .unwrap_or_else(|| panic!("Error retrieving deployed address"));
                let gas_used = rcpt.gas_used.unwrap_or_else(|| panic!("Error retrieving gas used"));
                let gas_price = rcpt
                    .effective_gas_price
                    .unwrap_or_else(|| panic!("Error retrieving gas price"));
                let block_number =
                    rcpt.block_number.unwrap_or_else(|| panic!("Error retrieving block number"));
                println!("+-------------------------------------------------+");
                println!("Contract successfully deployed to address: {:#?}", deployed_address);
                println!("Transaction Hash: {:#?}", deploy.hash());
                println!("Gas used: {:#?}", gas_used);
                println!("Effective gas price: {:#?}", gas_price);
                println!("Block Number: {:#?}", block_number);
                println!("+-------------------------------------------------+");
            }
            Err(e) => println!("{:#?}, error", e),
        }

        Ok(())
    }

    fn get_constructor_args(&self, abi: &Abi) -> Vec<Token> {
        let params = match abi.constructor {
            Some(ref v) => {
                let constructor_args =
                    if let Some(ref constructor_args_path) = self.constructor_args_path {
                        read_constructor_args_file(constructor_args_path.to_path_buf()).unwrap()
                    } else {
                        self.constructor_args.clone()
                    };
                self.parse_constructor_args(v, &constructor_args).unwrap()
            }
            None => vec![],
        };
        params
    }

    fn get_abi_from_contract(
        project: &Project,
        contract_info: &ContractInfo,
    ) -> Result<Value, serde_json::Error> {
        let output_path = Self::get_path_for_contract_output(project, contract_info);
        let contract_output = Self::get_contract_output(output_path);
        serde_json::from_value(
            contract_output[contract_info.path.as_ref().unwrap()][&contract_info.name]["abi"]
                .clone(),
        )
    }

    fn get_bytecode_from_contract(
        project: &Project,
        contract_info: &ContractInfo,
    ) -> Result<Bytes, serde_json::Error> {
        let output_path = Self::get_path_for_contract_output(project, contract_info);
        println!("{:#?}, output_path", output_path);
        let contract_output = Self::get_contract_output(output_path);
        // println!("{:#?}, contract_output", contract_output);
        println!("{:#?}, contract_info", contract_info);
        let temp = serde_json::from_value(
            contract_output[contract_info.path.as_ref().unwrap()][&contract_info.name]["evm"]
                ["bytecode"]["object"]
                .clone(),
        );
        println!("{:#?}, temp", temp);
        temp
    }

    fn get_contract_output(output_path: PathBuf) -> Value {
        let data = fs::read_to_string(output_path).expect("Unable to read file");
        let res: serde_json::Value = serde_json::from_str(&data).expect("Unable to parse");
        res["contracts"].clone()
    }

    fn get_path_for_contract_output(project: &Project, contract_info: &ContractInfo) -> PathBuf {
        let filepath = contract_info.path.clone().unwrap();
        let filename = filepath.split('/').last().unwrap();
        project.paths.artifacts.join(filename).join("artifacts.json")
    }

    fn get_factory_dependencies(
        &self,
        project: &Project,
        mut factory_dep_vector: Vec<Vec<u8>>,
        fdep_contract_info: Vec<ContractInfo>,
    ) -> Vec<Vec<u8>> {
        for dep in fdep_contract_info.iter() {
            println!("{:#?}, dep", dep);
            let dep_bytecode = Self::get_bytecode_from_contract(&project, dep).unwrap();
            factory_dep_vector.push(dep_bytecode.to_vec());
        }
        factory_dep_vector
    }

    fn get_signer(private_key: H256, chain: &Chain) -> Signer<PrivateKeySigner> {
        let eth_signer = PrivateKeySigner::new(private_key);
        let signer_addy = PackedEthSignature::address_from_private_key(&private_key)
            .expect("Can't get an address from the private key");
        Signer::new(eth_signer, signer_addy, L2ChainId(chain.id().try_into().unwrap()))
    }

    fn parse_constructor_args(
        &self,
        constructor: &Constructor,
        constructor_args: &[String],
    ) -> eyre::Result<Vec<Token>> {
        let params = constructor
            .inputs
            .iter()
            .zip(constructor_args)
            .map(|(input, arg)| (&input.kind, arg.as_str()))
            .collect::<Vec<_>>();

        parse_tokens(params, true)
    }
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
