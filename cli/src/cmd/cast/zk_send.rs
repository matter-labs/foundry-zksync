/// This module handles transactions related to ZkSync. It provides functionality for sending transactions
/// and withdrawing from Layer 2 to Layer 1. The module also defines the command-line arguments for the
/// `cast zk-send` subcommand.
///
/// The module consists of the following components:
/// - Helper functions for interacting with ZkSync and Ethereum:
///   - `get_url_with_port`: Retrieves the URL with port from the `zk_deposit` module.
///   - `get_chain`, `get_private_key`, `get_rpc_url`: Retrieves chain, private key, and RPC URL from the `zk_utils` module.
/// - Struct `ZkSendTxArgs` representing the command-line arguments for the `cast zk-send` subcommand:
///   - `to`: The destination of the transaction. Accepts address or name.
///   - `sig`: Signature of the function to call when interacting with a contract.
///   - `args`: Arguments for the function being called.
///   - `withdraw`: Flag indicating if the transaction is a Layer 2 to Layer 1 withdrawal.
///   - `token`: Token to bridge in case of withdrawal. Defaults to ETH if not provided.
///   - `amount`: Amount of token to bridge in case of withdrawal.
///   - `tx`: Transaction options such as gas price, nonce, etc.
///   - `eth`: Ethereum options such as sender's address, private key, etc.
/// - Implementation of the `ZkSendTxArgs` struct with methods:
///   - `run`: Executes the command-line arguments, loads the configuration, retrieves private key and RPC URL,
///     prepares and sends the transaction, and handles withdrawals.
///   - `print_receipt`: Prints the receipt of the transaction, including transaction hash, gas used, effective gas price,
///     block number, and deployed contract address.
///   - `get_signer`: Creates a signer from the private key and chain.
///   - `get_to_address`: Retrieves the recipient address of the transaction.
/// - Helper functions:
///   - `parse_decimal_u256`: Parses a decimal string into a `U256` number.
///   - `decode_hex`: Decodes a hexadecimal string into a byte vector.
///
/// Usage:
/// The `ZkSendTxArgs` struct is used to define and parse command-line arguments for the `cast zk-send` command.
/// It provides the `run` method to execute the transaction and the `print_receipt` method to print the transaction receipt.
///
/// The `run` method processes the command-line arguments, loads the configuration, retrieves the private key and RPC URL,
/// prepares the transaction, and sends it. If the transaction is a Layer 2 to Layer 1 withdrawal, it handles the withdrawal operation.
/// The method returns an `eyre::Result` indicating the success or failure of the transaction.
///
/// The `print_receipt` method extracts relevant information from the transaction receipt and prints it to the console.
/// This includes the transaction hash, gas used, effective gas price, block number, and deployed contract address, if applicable.
use crate::cmd::cast::zk_utils::zk_utils::{
    decode_hex, get_chain, get_private_key, get_rpc_url, get_signer,
};
use crate::opts::{cast::parse_name_or_address, EthereumOpts, TransactionOpts};
use cast::TxBuilder;
use clap::Parser;
use ethers::types::NameOrAddress;
use eyre::Context;
use foundry_common::try_get_http_provider;
use foundry_config::Config;
use zksync::{
    self,
    signer::Signer,
    types::{Address, TransactionReceipt, H160, U256},
    wallet,
};
use zksync_eth_signer::PrivateKeySigner;
use zksync_types::CONTRACT_DEPLOYER_ADDRESS;

/// CLI arguments for the `cast zk-send` subcommand.
///
/// This struct contains all the arguments and options that can be passed to the `zk-send` subcommand.
/// It has methods to run the subcommand and to print the receipt of the transaction.
#[derive(Debug, Parser)]
pub struct ZkSendTxArgs {
    /// The destination of the transaction.
    ///
    /// This field can be populated using the value parser `parse_name_or_address`.
    /// If not provided, the value is `None`.
    #[clap(
            help = "The destination of the transaction.",
            value_parser = parse_name_or_address,
            value_name = "TO"
        )]
    to: Option<NameOrAddress>,

    /// Signature of the function to call.
    /// This is used when the transaction involves calling a function on a contract.
    #[clap(help = "The signature of the function to call.", value_name = "SIG")]
    sig: Option<String>,

    /// Arguments for the function being called.
    /// These are passed in order to the function specified by `sig`.
    #[clap(help = "The arguments of the function to call.", value_name = "ARGS")]
    args: Vec<String>,

    /// Flag indicating whether the transaction is a Layer 2 to Layer 1 withdrawal.
    #[clap(
        long,
        short,
        help_heading = "Bridging options",
        help = "For L2 -> L1 withdrawals.",
        group = "bridging"
    )]
    withdraw: bool,

    /// Token to bridge in case of L2 to L1 withdrawal.
    /// If left blank, it will be treated as ETH.
    #[clap(
        long,
        help_heading = "Bridging options",
        help = "Token to bridge. Leave blank for ETH.",
        value_name = "TOKEN"
    )]
    token: Option<String>,

    /// Amount of token to bridge in case of L2 to L1 withdrawal.
    /// This is required when the `withdraw` flag is set.
    #[clap(
        long,
        short,
        help_heading = "Bridging options",
        help = "Amount of token to bridge. Required value when bridging",
        value_name = "AMOUNT",
        requires = "bridging",
        value_parser = parse_decimal_u256
    )]
    amount: Option<U256>,

    /// Options for the transaction such as gas price, nonce etc.
    #[clap(flatten)]
    tx: TransactionOpts,

    /// Ethereum related options such as sender's address, private key, etc.
    #[clap(flatten)]
    eth: EthereumOpts,
}

impl ZkSendTxArgs {
    /// Executes the arguments passed through the CLI.
    ///
    /// This function processes all the arguments and options passed through the CLI.
    /// It loads the configuration, retrieves the private key and RPC URL, prepares the transaction
    /// and sends it. It also handles the withdraw functionality.
    ///
    /// # Returns
    ///
    /// An `eyre::Result` which is:
    /// - Ok: If the transaction or withdraw operation is successful.
    /// - Err: If any error occurs during the operation.
    pub async fn run(self) -> eyre::Result<()> {
        let config = Config::load();

        let private_key = get_private_key(&self.eth.wallet.private_key)?;

        let rpc_url = get_rpc_url(&self.eth.rpc_url)?;

        let chain = get_chain(self.eth.chain)?;

        let signer: Signer<PrivateKeySigner> = get_signer(private_key, &chain);
        let provider = try_get_http_provider(config.get_rpc_url_or_localhost_http()?)?;
        let to_address = self.get_to_address();
        let sender = self.eth.sender().await;

        let wallet = wallet::Wallet::with_http_client(&rpc_url, signer);

        if self.withdraw {
            let token_address: Address = match &self.token {
                Some(token_addy) => {
                    let decoded = match decode_hex(token_addy) {
                        Ok(addy) => addy,
                        Err(e) => {
                            eyre::bail!("Error parsing token address: {e}, try removing the '0x'")
                        }
                    };
                    Address::from_slice(decoded.as_slice())
                }
                None => Address::zero(),
            };

            let amount = self
                .amount
                .expect("Amount was not provided. Use --amount flag (ex. --amount 1000000000 )");

            match wallet {
                Ok(w) => {
                    println!("Bridging assets....");
                    // Build Withdraw //
                    let tx = w
                        .start_withdraw()
                        .to(to_address)
                        .amount(amount)
                        .token(token_address)
                        .send()
                        .await
                        .unwrap();

                    let rcpt = match tx.wait_for_commit().await {
                        Ok(rcpt) => rcpt,
                        Err(e) => eyre::bail!("Transaction Error: {}", e),
                    };

                    self.print_receipt(&rcpt);
                }
                Err(e) => eyre::bail!("error wallet: {e:?}"),
            };
        } else {
            match wallet {
                Ok(w) => {
                    println!("Sending transaction....");

                    // Here we are constructing the parameters for the transaction
                    let sig = self.sig.as_ref().expect("Error: Function Signature is empty");
                    let params =
                        if !sig.is_empty() { Some((&sig[..], self.args.clone())) } else { None };

                    // Creating a new transaction builder
                    let mut builder =
                        TxBuilder::new(&provider, sender, self.to.clone(), chain, true).await?;

                    builder.args(params).await?;

                    let (tx, _func) = builder.build();
                    let encoded_function_call = tx.data().unwrap().to_vec();

                    let tx = w
                        .start_execute_contract()
                        .contract_address(to_address)
                        .calldata(encoded_function_call)
                        .send()
                        .await
                        .wrap_err("Failed to execute transaction")?;

                    let rcpt = match tx.wait_for_commit().await {
                        Ok(rcpt) => rcpt,
                        Err(e) => eyre::bail!("Transaction Error: {}", e),
                    };

                    self.print_receipt(&rcpt);
                }
                Err(e) => eyre::bail!("error wallet: {e:?}"),
            };
        }

        Ok(())
    }

    /// Prints the receipt of the transaction.
    ///
    /// This function extracts the transaction hash, gas used, effective gas price, and block number
    /// from the receipt and prints them. It also prints the address of the deployed contract, if any.
    ///
    /// # Arguments
    ///
    /// * `rcpt` - A reference to the `TransactionReceipt`.
    fn print_receipt(&self, rcpt: &TransactionReceipt) {
        let gas_used = rcpt.gas_used.expect("Error retrieving gas used");
        let gas_price = rcpt.effective_gas_price.expect("Error retrieving gas price");
        let block_number = rcpt.block_number.expect("Error retrieving block number");

        println!("+-------------------------------------------------+");
        println!("Transaction Hash: {:#?}", rcpt.transaction_hash);
        println!("Gas used: {:#?}", gas_used);
        println!("Effective gas price: {:#?}", gas_price);
        println!("Block Number: {:#?}", block_number);
        println!("+-------------------------------------------------+");

        // This will display a deployed contract address if one was deployed via zksend
        for log in &rcpt.logs {
            if log.address == CONTRACT_DEPLOYER_ADDRESS {
                let deployed_address = log.topics.get(3).unwrap();
                let deployed_address = Address::from(*deployed_address);
                println!("Deployed contract address: {:#?}", deployed_address);
                println!("+-------------------------------------------------+");
            }
        }
    }

    /// Gets the recipient address of the transaction.
    ///
    /// If the `to` field is `None`, it will panic with the message "Enter TO: Address".
    ///
    /// # Returns
    ///
    /// A `H160` object that represents the recipient's address.
    fn get_to_address(&self) -> H160 {
        let to = self.to.as_ref().expect("Enter TO: Address");
        let deployed_contract = to.as_address().expect("Invalid address").as_bytes();
        zksync_utils::be_bytes_to_safe_address(&deployed_contract).unwrap()
    }
}

/// Parses a decimal string into a U256 number.
///
/// If the string cannot be parsed into a U256, an error message is returned.
///
/// # Arguments
///
/// * `s` - A string that represents a decimal number.
///
/// # Returns
///
/// A `Result` which is:
/// - Ok: Contains the parsed U256 number.
/// - Err: Contains an error message indicating that the parsing failed.
fn parse_decimal_u256(s: &str) -> Result<U256, String> {
    match U256::from_dec_str(s) {
        Ok(value) => Ok(value),
        Err(e) => Err(format!("Failed to parse decimal number: {}", e)),
    }
}
