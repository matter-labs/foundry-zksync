//! # ZKSync Contract Deployment Module
//!
//! This module contains various utilities and helpers to facilitate interactions with
//! zkSync contracts on the Ethereum network. It provides an abstraction over raw Ethereum
//! transactions, allowing developers to interact with the contracts in a type-safe and
//! efficient manner. The primary features of this module include:
//!
//! 1. **Contract Deployment:** Provides a method to deploy zkSync contracts on the Ethereum network.
//! 2. **Contract Interaction:** Provides various methods to interact with deployed contracts, such as calling contract methods, parsing contract ABIs, and handling events.
//! 3. **Signing Transactions:** Provides a helper to create a signer for transactions from a provided private key.
//!
//! To ensure ease of use, this module relies on several other libraries and crates such as web3,
//! ethabi, zksync_types, and foundry_common. It uses web3 to connect to Ethereum nodes, ethabi
//! to parse contract ABIs, zksync_types for zkSync specific types and foundry_common for common
//! utilities like parsing tokens and handling function parameters.
//!
//! This module plays a crucial role in the zkSync ecosystem by enabling developers to seamlessly
//! deploy and interact with zkSync contracts.
//!
//! All the functions in this module return `eyre::Result`, which is a rich error type that makes it easy to build
//! context-aware error reports.
use crate::{
    cmd::{
        cast::zk_deposit::get_url_with_port, forge::build::CoreBuildArgs,
        read_constructor_args_file,
    },
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
use zksync::zksync_types::{L2ChainId, PackedEthSignature};
use zksync::{self, signer::Signer, wallet};
use zksync_eth_signer::PrivateKeySigner;

/// CLI arguments for `forge zk-create`.
/// Struct `ZkCreateArgs` encapsulates the arguments necessary for creating a new zkSync contract.
///
/// This struct is used to cleanly pass the required parameters for contract deployment to the
/// `create` function. It ensures type safety and reduces the chance of passing incorrect or
/// mismatched parameters.
///
/// The `ZkCreateArgs` struct has the following fields:
///
/// * `constructor_args`: This field represents the arguments for the zkSync contract constructor.
///   The arguments are represented as a vector of `Token` values. Each `Token` corresponds to
///   an argument of the contract's constructor.
///
/// * `encoded_constructor_args`: This is the hex encoded string representation of the constructor
///   arguments. It is used when deploying the contract on the Ethereum network.
///
/// * `bytecode`: The bytecode of the zkSync contract that is to be deployed. This is a compiled
///   version of the contract's source code.
///
/// * `private_key`: The private key used for signing the contract deployment transaction. It
///   is the private key of the account that will own the deployed contract.
///
/// * `chain_id`: The ID of the Ethereum network chain where the contract is to be deployed.
///   Different chain IDs represent different Ethereum networks (e.g., mainnet, testnet).
#[derive(Debug, Clone, Parser)]
#[clap(next_help_heading = "ZkCreate options", about = None)]
pub struct ZkCreateArgs {
    /// The contract identifier in the form `<path>:<contractname>`.
    #[clap(
        help = "The contract identifier in the form `<path>:<contractname>`.",
        value_name = "CONTRACT"
    )]
    contract: ContractInfo,

    /// The constructor arguments.
    #[clap(
        long,
        num_args(1..),
        help = "The constructor arguments.",
        name = "constructor_args",
        conflicts_with = "constructor_args_path",
        value_name = "ARGS"
    )]
    constructor_args: Vec<String>,

    /// The path to a file containing the constructor arguments.
    #[clap(
        long,
        help = "The path to a file containing the constructor arguments.",
        value_hint = ValueHint::FilePath,
        name = "constructor_args_path",
        conflicts_with = "constructor_args",
        value_name = "FILE"
    )]
    constructor_args_path: Option<PathBuf>,

    /// The factory dependencies in the form `<path>:<contractname>`.
    #[clap(
        long,
        num_args(1..),
        help_heading = "ZkSync Features",
        help = "The factory dependencies in the form `<path>:<contractname>`.",
        value_name = "FACTORY-DEPS"
    )]
    factory_deps: Option<Vec<ContractInfo>>,

    /// Core build arguments.
    #[clap(flatten)]
    opts: CoreBuildArgs,

    /// Transaction options, such as gas price and gas limit.
    #[clap(flatten)]
    tx: TransactionOpts,

    /// Ethereum-specific options, such as the network and wallet.
    #[clap(flatten)]
    eth: EthereumOpts,
}

impl ZkCreateArgs {
    /// Executes the command to create a contract.
    ///
    /// # Procedure
    /// 1. Retrieves private key, RPC URL, and chain information from the current instance.
    /// 2. It then sets up the project and artifact paths.
    /// 3. Retrieves the bytecode of the contract.
    /// 4. If factory dependencies are present, they are processed.
    /// 5. A signer is created using the private key and chain.
    /// 6. The ABI of the contract is obtained.
    /// 7. The constructor arguments are encoded.
    /// 8. A wallet is set up using the signer and the RPC URL.
    /// 9. The contract deployment is started.
    /// 10. If deployment is successful, the contract address, transaction hash, gas used, gas price, and block number are printed to the console.
    pub async fn run(self) -> eyre::Result<()> {
        //get private key
        let private_key = self.get_private_key()?;

        let rpc_url = self.get_rpc_url()?;

        let chain = self.get_chain()?;

        // get project
        let mut project = self.opts.project()?;
        // set out folder path
        project.paths.artifacts = project.paths.root.join("zkout");

        let bytecode = match Self::get_bytecode_from_contract(&project, &self.contract) {
            Ok(bytecode) => bytecode,
            Err(e) => {
                eyre::bail!("Error getting bytecode from contract: {}", e);
            }
        };

        //check for additional factory deps
        let mut factory_deps = Vec::new();
        if let Some(fdep_contract_info) = &self.factory_deps {
            factory_deps =
                self.get_factory_dependencies(&project, factory_deps, fdep_contract_info);
        }

        // get signer
        let signer = Self::get_signer(private_key, &chain);

        // get abi
        let abi = match Self::get_abi_from_contract(&project, &self.contract) {
            Ok(abi) => abi,
            Err(e) => {
                eyre::bail!("Error gettting ABI from contract: {}", e);
            }
        };

        let contract = match serde_json::from_value(abi) {
            Ok(contract) => contract,
            Err(e) => {
                eyre::bail!("Error converting json abi to Contract ABI: {}", e);
            }
        };

        // encode constructor args
        let encoded_args = encode(self.get_constructor_args(&contract).as_slice());

        let wallet = wallet::Wallet::with_http_client(&rpc_url, signer);
        let deployer_builder = match &wallet {
            Ok(w) => w.start_deploy_contract(),
            Err(e) => eyre::bail!("error wallet: {e:?}"),
        };

        let deployer = deployer_builder
            .bytecode(bytecode.to_vec())
            .factory_deps(factory_deps)
            .constructor_calldata(encoded_args);

        // TODO: could be useful as a flag --estimate-gas
        // let est_gas = deployer.estimate_fee(None).await;
        // println!("{:#?}, est_gas", est_gas);

        println!("Deploying contract...");
        match deployer.send().await {
            Ok(tx_handle) => {
                let rcpt = match tx_handle.wait_for_commit().await {
                    Ok(rcpt) => rcpt,
                    Err(e) => eyre::bail!("Transaction Error: {}", e),
                };

                let deployed_address =
                    rcpt.contract_address.expect("Error retrieving deployed address");
                let gas_used = rcpt.gas_used.expect("Error retrieving gas used");
                let gas_price = rcpt.effective_gas_price.expect("Error retrieving gas price");
                let block_number = rcpt.block_number.expect("Error retrieving block number");

                println!("+-------------------------------------------------+");
                println!("Contract successfully deployed to address: {:#?}", deployed_address);
                println!("Transaction Hash: {:#?}", tx_handle.hash());
                println!("Gas used: {:#?}", gas_used);
                println!("Effective gas price: {:#?}", gas_price);
                println!("Block Number: {:#?}", block_number);
                println!("+-------------------------------------------------+");
            }
            Err(e) => eyre::bail!("{:#?}, error", e),
        };

        Ok(())
    }

    /// This function gets the RPC URL for Ethereum.
    ///
    /// If the `eth.rpc_url` is `None`, an error is returned.
    ///
    /// # Returns
    ///
    /// A `Result` which is:
    /// - Ok: Contains the RPC URL as a String.
    /// - Err: Contains an error message indicating that the RPC URL was not provided.
    fn get_rpc_url(&self) -> eyre::Result<String> {
        match &self.eth.rpc_url {
            Some(url) => {
                let rpc_url = get_url_with_port(url)
                    .ok_or_else(|| eyre::Report::msg("Invalid RPC_URL"))?;
                Ok(rpc_url)
            },
            None => Err(eyre::Report::msg("RPC URL was not provided. Try using --rpc-url flag or environment variable 'ETH_RPC_URL= '")),
        }
    }

    /// Gets the private key from the Ethereum options.
    ///
    /// If the `eth.wallet.private_key` is `None`, an error is returned.
    ///
    /// # Returns
    ///
    /// A `Result` which is:
    /// - Ok: Contains the private key as `H256`.
    /// - Err: Contains an error message indicating that the private key was not provided.
    fn get_private_key(&self) -> eyre::Result<H256> {
        match &self.eth.wallet.private_key {
            Some(pkey) => {
                let val = decode_hex(pkey)
                    .map_err(|e| eyre::Report::msg(format!("Error parsing private key: {}", e)))?;
                Ok(H256::from_slice(&val))
            }
            None => {
                Err(eyre::Report::msg("Private key was not provided. Try using --private-key flag"))
            }
        }
    }

    /// Gets the chain from the Ethereum options.
    ///
    /// If the `eth.chain` is `None`, an error is returned.
    ///
    /// # Returns
    ///
    /// A `Result` which is:
    /// - Ok: Contains the chain as `Chain`.
    /// - Err: Contains an error message indicating that the chain was not provided.
    fn get_chain(&self) -> eyre::Result<Chain> {
        match &self.eth.chain {
            Some(chain) => Ok(chain.clone()),
            None => Err(eyre::Report::msg("Chain was not provided. Use --chain flag (ex. --chain 270 ) \nor environment variable 'CHAIN= ' (ex.'CHAIN=270')")),
        }
    }

    /// This function retrieves the constructor arguments for the contract.
    ///
    /// # Returns
    /// A vector of `Token` which represents the constructor arguments.
    fn get_constructor_args(&self, abi: &Abi) -> Vec<Token> {
        match &abi.constructor {
            Some(v) => {
                let constructor_args =
                    if let Some(ref constructor_args_path) = self.constructor_args_path {
                        read_constructor_args_file(constructor_args_path.to_path_buf()).unwrap()
                    } else {
                        self.constructor_args.clone()
                    };
                self.parse_constructor_args(v, &constructor_args).unwrap()
            }
            None => vec![],
        }
    }

    /// This function retrieves the ABI from the contract.
    ///
    /// # Errors
    /// If there is an error in retrieving or parsing the ABI, it returns a serde_json::Error.
    ///
    /// # Returns
    /// A `Result` which is:
    /// - Ok: Contains the ABI as a serde_json::Value.
    /// - Err: Contains a serde_json::Error.
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

    /// This function retrieves the bytecode from the contract.
    ///
    /// # Procedure
    /// 1. Retrieves the contract info, checks if the contract's bytecode exists.
    /// 2. If the bytecode exists, it is decoded from hexadecimal representation into bytes.
    ///
    /// # Errors
    /// If there is an error in retrieving or decoding the bytecode, it returns an Error.
    ///
    /// # Returns
    /// A `Result` which is:
    /// - Ok: Contains the bytecode as a Bytes.
    /// - Err: Contains an error message indicating a problem with retrieving or decoding the bytecode.
    fn get_bytecode_from_contract(
        project: &Project,
        contract_info: &ContractInfo,
    ) -> Result<Bytes, serde_json::Error> {
        let output_path = Self::get_path_for_contract_output(project, contract_info);
        let contract_output = Self::get_contract_output(output_path);
        let byte_code = serde_json::from_value(
            contract_output[contract_info.path.as_ref().unwrap()][&contract_info.name]["evm"]
                ["bytecode"]["object"]
                .clone(),
        );
        byte_code
    }

    /// This function retrieves the contract output.
    ///
    /// # Arguments
    ///
    /// * `output_path` - A `PathBuf` that represents the path to the contract output file.
    ///
    /// # Procedure
    ///
    /// 1. Reads the contract output file into a string.
    /// 2. Converts the string into a `serde_json::Value`.
    /// 3. Returns the "contracts" field from the JSON value.
    ///
    /// # Returns
    ///
    /// A `serde_json::Value` that represents the contract output.
    fn get_contract_output(output_path: PathBuf) -> Value {
        let data = fs::read_to_string(output_path).expect("Unable to read file");
        let res: serde_json::Value = serde_json::from_str(&data).expect("Unable to parse");
        res["contracts"].clone()
    }

    /// This function retrieves the path for the contract output.
    ///
    /// # Arguments
    ///
    /// * `project` - A `Project` instance that represents the current Solidity project.
    /// * `contract_info` - A `ContractInfo` instance that contains information about the contract.
    ///
    /// # Procedure
    ///
    /// 1. Retrieves the contract file path from `contract_info`.
    /// 2. Retrieves the contract file name from the file path.
    /// 3. Joins the artifacts path of the project with the contract file name.
    /// 4. Joins the resulting path with "artifacts.json" to create the path to the contract output.
    ///
    /// # Returns
    ///
    /// A `PathBuf` that represents the path to the contract output.
    fn get_path_for_contract_output(project: &Project, contract_info: &ContractInfo) -> PathBuf {
        let filepath = contract_info.path.clone().unwrap();
        let filename = filepath.split('/').last().unwrap();
        project.paths.artifacts.join(filename).join("artifacts.json")
    }

    /// This function retrieves the factory dependencies.
    ///
    /// # Arguments
    ///
    /// * `project` - A `Project` instance that represents the current Solidity project.
    /// * `factory_dep_vector` - A vector that contains the bytecode of each factory dependency contract.
    /// * `fdep_contract_info` - A vector of `ContractInfo` instances that contain information about each factory dependency contract.
    ///
    /// # Procedure
    ///
    /// 1. Iterates over each factory dependency contract in `fdep_contract_info`.
    /// 2. For each contract, retrieves its bytecode and appends it to `factory_dep_vector`.
    ///
    /// # Returns
    ///
    /// A vector of vectors of bytes that represents the bytecode of each factory dependency contract.
    fn get_factory_dependencies(
        &self,
        project: &Project,
        mut factory_dep_vector: Vec<Vec<u8>>,
        fdep_contract_info: &Vec<ContractInfo>,
    ) -> Vec<Vec<u8>> {
        for dep in fdep_contract_info.iter() {
            let dep_bytecode = Self::get_bytecode_from_contract(&project, dep).unwrap();
            factory_dep_vector.push(dep_bytecode.to_vec());
        }
        factory_dep_vector
    }

    /// Generates a Signer for the transaction using the provided private key and chain.
    ///
    /// This function creates a `Signer` instance for signing transactions on the zkSync network.
    /// It uses the private key to create an `eth_signer` and derives the associated address.
    /// The `Signer` is then initialized with the `eth_signer`, the derived address, and the L2ChainId
    /// derived from the `Chain` instance.
    ///
    /// # Parameters
    /// - `private_key`: A H256 type private key used to sign the transactions.
    /// - `chain`: A reference to a `Chain` instance which contains chain-specific configuration.
    ///
    /// # Returns
    /// A `Signer` instance for signing transactions.
    fn get_signer(private_key: H256, chain: &Chain) -> Signer<PrivateKeySigner> {
        let eth_signer = PrivateKeySigner::new(private_key);
        let signer_addy = PackedEthSignature::address_from_private_key(&private_key)
            .expect("Can't get an address from the private key");
        Signer::new(eth_signer, signer_addy, L2ChainId(chain.id().try_into().unwrap()))
    }

    /// Parses the constructor arguments based on the ABI inputs.
    ///
    /// This function parses the constructor arguments provided by the user, matching them with
    /// the ABI inputs of the constructor. The parsing process ensures that the arguments are
    /// in the right format and type as specified by the contract's ABI. It uses the `parse_tokens`
    /// function from the `foundry_common` crate to facilitate this parsing.
    ///
    /// # Parameters
    /// - `constructor`: The ABI of the contract constructor. It contains the input parameters' information.
    /// - `constructor_args`: A slice of strings that represents the arguments provided by the user.
    ///
    /// # Returns
    /// A `Result` which is:
    /// - Ok: Contains a vector of `Token` objects that represent the parsed arguments.
    /// - Err: Contains an eyre::Report error in case of parsing failure.
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
