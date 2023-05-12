// cast send subcommands
use crate::cmd::cast::zk_deposit::get_url_with_port;
use crate::opts::{cast::parse_name_or_address, EthereumOpts, TransactionOpts};
use cast::TxBuilder;
use clap::Parser;
use ethers::types::NameOrAddress;
use foundry_common::try_get_http_provider;
use foundry_config::{Chain, Config};
use zksync::{
    self,
    signer::Signer,
    types::{Address, TransactionReceipt, H160, H256, U256},
    wallet,
    zksync_types::{L2ChainId, PackedEthSignature},
};
use zksync_eth_signer::PrivateKeySigner;
use zksync_types::CONTRACT_DEPLOYER_ADDRESS;

/// CLI arguments for `cast zk-send`.
#[derive(Debug, Parser)]
pub struct ZkSendTxArgs {
    #[clap(
            help = "The destination of the transaction.",
            value_parser = parse_name_or_address,
            value_name = "TO"
        )]
    to: Option<NameOrAddress>,

    #[clap(help = "The signature of the function to call.", value_name = "SIG")]
    sig: Option<String>,

    #[clap(help = "The arguments of the function to call.", value_name = "ARGS")]
    args: Vec<String>,

    #[clap(
        long,
        short,
        help_heading = "Bridging options",
        help = "For L2 -> L1 withdrawals.",
        group = "bridging"
    )]
    withdraw: bool,

    #[clap(
        long,
        help_heading = "Bridging options",
        help = "Token to bridge. Leave blank for ETH.",
        value_name = "TOKEN"
    )]
    token: Option<String>,

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

    #[clap(flatten)]
    tx: TransactionOpts,

    #[clap(flatten)]
    eth: EthereumOpts,

    #[clap(
        short,
        long,
        help = "The number of confirmations until the receipt is fetched.",
        default_value = "1",
        value_name = "CONFIRMATIONS"
    )]
    confirmations: usize,
    #[clap(long = "json", short = 'j', help_heading = "Display options")]
    to_json: bool,
    #[clap(
        long = "resend",
        help = "Reuse the latest nonce for the sender account.",
        conflicts_with = "nonce"
    )]
    resend: bool,
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

        let private_key = self.get_private_key()?;

        let rpc_url = self.get_rpc_url()?;

        let chain = self.get_chain()?;

        let signer = Self::get_signer(private_key, &chain);
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
                        .unwrap();

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

        for log in &rcpt.logs {
            if log.address == CONTRACT_DEPLOYER_ADDRESS {
                let deployed_address = log.topics.get(3).unwrap();
                let deployed_address = Address::from(*deployed_address);
                println!("Deployed contract address: {:#?}", deployed_address);
                println!("+-------------------------------------------------+");
            }
        }
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

    /// Creates a signer from the private key and the chain.
    ///
    /// # Arguments
    ///
    /// * `private_key` - A `H256` that represents the private key.
    /// * `chain` - A reference to `Chain` that represents the chain.
    ///
    /// # Returns
    ///
    /// A `Signer` object that can be used to sign transactions.
    fn get_signer(private_key: H256, chain: &Chain) -> Signer<PrivateKeySigner> {
        let eth_signer = PrivateKeySigner::new(private_key);
        let signer_addy = PackedEthSignature::address_from_private_key(&private_key)
            .expect("Can't get an address from the private key");
        Signer::new(eth_signer, signer_addy, L2ChainId(chain.id().try_into().unwrap()))
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

use std::num::ParseIntError;
pub fn decode_hex(s: &str) -> std::result::Result<Vec<u8>, ParseIntError> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16)).collect()
}
