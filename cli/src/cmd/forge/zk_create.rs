//! Create command
use super::verify;
use crate::{
    cmd::{
        forge::build::CoreBuildArgs, read_constructor_args_file, remove_contract, retry::RetryArgs,
        LoadConfig,
    },
    opts::{EthereumOpts, TransactionOpts, WalletType},
};
use clap::{Parser, ValueHint};
use ethers::{
    abi::{Abi, Constructor, Token},
    prelude::{artifacts::BytecodeObject, ContractFactory, Middleware},
    solc::{info::ContractInfo, utils::canonicalized, Project},
    types::{transaction::eip2718::TypedTransaction, Bytes, Chain},
};
use rustc_hex::ToHex;
use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::Arc};
use tracing::log::trace;
use zk_evm::k256::pkcs8::Error;

//for zksync
use crate::cmd::forge::zksync_deploy;

use zksync::types::H256;
use zksync::zksync_eth_signer::PrivateKeySigner;
use zksync::zksync_types::{L2ChainId, PackedEthSignature};
use zksync::{self, signer::Signer, wallet};
use zksync_types::{Address, CONTRACT_DEPLOYER_ADDRESS};

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

    #[clap(flatten)]
    opts: CoreBuildArgs,

    #[clap(flatten)]
    tx: TransactionOpts,

    #[clap(flatten)]
    eth: EthereumOpts,

    #[clap(
        long,
        num_args(1..),
        help = "The factory dependencies in the form `<path>:<contractname>`.",
        value_name = "FACTORY-DEPS"
    )]
    factory_deps: Option<Vec<ContractInfo>>,
}

impl ZkCreateArgs {
    /// Executes the command to create a contract
    pub async fn run(mut self) -> eyre::Result<()> {
        println!("{:#?}, ZkCreateArgs ---->>>", self);

        // get project and set paths
        let mut project = self.opts.project()?;
        project.paths.artifacts = project.paths.root.join("zkout");

        let mut output_path = Self::get_path_for_contract_output(&project, &self.contract);

        println!("{:#?}, project ---->>>", project);

        let contracts_ouput = self.get_contract_output(output_path);
        // let bytecode = self.get_bytecode(contracts_ouput.clone()).unwrap();
        let bytecode = get_bytecode_from_contract(&self.contract, contracts_ouput.clone()).unwrap();

        println!("{:#?}, bytecode", bytecode);
        //-----------------------//
        // initial factory dep
        let mut factory_deps = vec![bytecode.to_vec()];

        // //check for additional factory deps
        if let Some(fdep_contract_info) = self.factory_deps.clone() {
            self.get_additional_factory_dependencies(
                &project,
                &mut factory_deps,
                fdep_contract_info,
            );
        }

        println!("{:#?}, factory_deps", factory_deps);

        let signer = self.get_signer();

        Ok(())
    }

    pub fn get_contract_output(&self, output_path: PathBuf) -> Value {
        //get standard json output
        let data = fs::read_to_string(output_path).expect("Unable to read file");
        //convert to json Value
        let res: serde_json::Value = serde_json::from_str(&data).expect("Unable to parse");
        res["contracts"].clone()
    }

    fn get_bytecode(&self, contract_out: Value) -> Result<Bytes, serde_json::Error> {
        // get bytecode
        serde_json::from_value(
            contract_out[&self.contract.path.clone().unwrap()][&self.contract.name]["evm"]
                ["bytecode"]["object"]
                .clone(),
        )
    }
    //do this
    fn get_path_for_contract_output(project: &Project, contract_info: &ContractInfo) -> PathBuf {
        let mut filename = contract_info.path.clone().unwrap();
        let abc = filename.split('/').last().unwrap();
        project.paths.artifacts.join(abc).join("artifacts.json")
    }

    fn get_additional_factory_dependencies(
        &self,
        project: &Project,
        factory_dep_vector: &mut Vec<Vec<u8>>,
        fdep_contract_info: Vec<ContractInfo>,
    ) {
        for dep in fdep_contract_info.iter() {
            let mut output_path = Self::get_path_for_contract_output(&project, dep);
            let output = self.get_contract_output(output_path);
            let dep_bytecode = get_bytecode_from_contract(dep, output).unwrap();
            factory_dep_vector.push(dep_bytecode.to_vec());
        }
    }

    fn get_signer(&self) -> Signer<PrivateKeySigner> {
        // get signer
        let private_key =
            H256::from_slice(&decode_hex(&self.eth.wallet.private_key.clone().unwrap()).unwrap());
        let eth_signer = PrivateKeySigner::new(private_key);
        let signer_addy = PackedEthSignature::address_from_private_key(&private_key)
            .expect("Can't get an address from the private key");
        Signer::new(
            eth_signer,
            signer_addy,
            L2ChainId(self.eth.chain.unwrap().id().try_into().unwrap()),
        )
    }
}

pub fn get_bytecode_from_contract(
    contract_info: &ContractInfo,
    contract_out: Value,
) -> Result<Bytes, serde_json::Error> {
    // get bytecode
    serde_json::from_value(
        contract_out[contract_info.path.as_ref().unwrap()][&contract_info.name]["evm"]["bytecode"]
            ["object"]
            .clone(),
    )
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
