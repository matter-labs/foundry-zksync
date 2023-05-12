/*
This module is responsible for handling transactions related to ZkSync.
It defines the CLI arguments for the `cast zk-deposit` command and provides functionality
for depositing assets into a zkSync contract.
*/
use crate::opts::{cast::parse_name_or_address, EthereumOpts, TransactionOpts};
use clap::Parser;
use ethers::types::NameOrAddress;
use foundry_config::Chain;
use url::Url;
use zksync::{
    self,
    signer::Signer,
    types::{Address, H160, H256, U256},
    wallet,
    zksync_types::{L2ChainId, PackedEthSignature},
};
use zksync_eth_signer::PrivateKeySigner;

/// Struct to represent the command line arguments for the `cast zk-deposit` command.
///
/// `ZkDepositTxArgs` contains parameters to be passed via the command line for the `cast zk-deposit`
/// operation. These include the destination of the transaction, the amount to deposit, an optional
/// bridge address, an optional operator tip, the zkSync RPC endpoint, and the token to bridge.
#[derive(Debug, Parser)]
pub struct ZkDepositTxArgs {
    /// The destination address of the transaction.
    /// This can be either a name or an address.
    #[clap(
            help = "The destination of the transaction.",
             value_parser = parse_name_or_address,
            value_name = "TO"
        )]
    to: NameOrAddress,

    /// The amount of the token to deposit.
    #[clap(
        long,
        short,
        help = "Amount of token to deposit.",
        value_name = "AMOUNT",
        value_parser = parse_decimal_u256
    )]
    amount: U256,

    /// An optional address of a custom bridge to call.
    #[clap(help = "The address of a custom bridge to call.", value_name = "BRIDGE")]
    bridge_address: Option<Address>,

    /// An optional tip that the user can choose to pay in addition to the regular transaction fee.
    #[clap(
        help = "Optional fee that the user can choose to pay in addition to the regular transaction fee.",
        value_name = "TIP"
    )]
    operator_tip: Option<U256>,

    /// The zkSync RPC endpoint.
    /// Can be provided via the environment variable 'ZKSYNC_RPC_URL' or the command line.
    #[clap(
        env = "ZKSYNC_RPC_URL",
        long,
        short = 'z',
        help = "The zkSync RPC endpoint.",
        value_name = "L2URL"
    )]
    l2_url: String,

    /// An optional token to bridge. If not specified, ETH is assumed.
    #[clap(long, help = "Token to bridge. Leave blank for ETH.", value_name = "TOKEN")]
    token: Option<Address>,

    /// Transaction options, such as gas price and gas limit.
    #[clap(flatten)]
    tx: TransactionOpts,

    /// Ethereum-specific options, such as the network and wallet.
    #[clap(flatten)]
    eth: EthereumOpts,
}

impl ZkDepositTxArgs {
    /// Executes the deposit transaction based on the provided command line arguments.
    ///
    /// This function first gets the private key from the Ethereum options and the chain.
    /// Then, it creates a new wallet using the zkSync HTTP client and signer.
    /// Finally, it deposits the specified amount to the target address and prints the transaction hash.
    ///
    /// # Returns
    ///
    /// A `Result` which is:
    /// - Ok: If the deposit transaction is successfully completed.
    /// - Err: If an error occurred during the execution of the deposit transaction.
    pub async fn run(self) -> eyre::Result<()> {
        //get private key
        let private_key = self.get_private_key()?;

        let rpc_url = self
            .eth
            .rpc_url()
            .expect("RPC URL was not provided. \nTry using --rpc-url flag or environment variable 'ETH_RPC_URL= '");

        let l2_url = get_url_with_port(&self.l2_url).expect("Invalid L2_RPC_URL");

        let chain = self.get_chain()?;

        let signer = Self::get_signer(private_key, &chain);

        let wallet = wallet::Wallet::with_http_client(&l2_url, signer);

        let to_address = self.get_to_address();
        let token_address: Address = match self.token {
            Some(token_addy) => token_addy,
            None => Address::zero(),
        };

        match wallet {
            Ok(w) => {
                println!("Bridging assets....");
                let eth_provider = w.ethereum(rpc_url).await.map_err(|e| e)?;
                let tx_hash = eth_provider
                    .deposit(
                        token_address,
                        self.amount,
                        to_address,
                        self.operator_tip,
                        self.bridge_address,
                        None,
                    )
                    .await?;

                println!("Transaction Hash: {:#?}", tx_hash);
            }
            Err(e) => eyre::bail!("Failed to download the file: {}", e),
        }

        Ok(())
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

    /// Creates a new signer using the given private key and chain.
    ///
    /// The function uses the provided private key to create an instance of `PrivateKeySigner`.
    /// It then uses this signer and the address derived from the private key to create a new `Signer`.
    ///
    /// # Parameters
    ///
    /// - `private_key`: The private key used to sign transactions.
    /// - `chain`: The chain associated with the signer.
    ///
    /// # Returns
    ///
    /// A `Signer<PrivateKeySigner>` instance.
    fn get_signer(private_key: H256, chain: &Chain) -> Signer<PrivateKeySigner> {
        let eth_signer = PrivateKeySigner::new(private_key);
        let signer_addy = PackedEthSignature::address_from_private_key(&private_key)
            .expect("Can't get an address from the private key");
        Signer::new(eth_signer, signer_addy, L2ChainId(chain.id().try_into().unwrap()))
    }

    /// Retrieves the 'to' address from the command line arguments.
    ///
    /// The 'to' address is expected to be a command line argument (`to`).
    /// If it is not provided, the function will return an error.
    ///
    /// # Returns
    ///
    /// A `H160` which represents the 'to' address.
    fn get_to_address(&self) -> H160 {
        let to = self.to.as_address().expect("Please enter TO address.");
        let deployed_contract = to.as_bytes();
        zksync_utils::be_bytes_to_safe_address(&deployed_contract).unwrap()
    }
}

/// Parses a URL string and attaches a default port if one is not specified.
///
/// This function takes a URL string as input and attempts to parse it.
/// If the URL string is not a valid URL, the function returns `None`.
/// If the URL is valid and has a specified port, the function returns the URL as is.
/// If the URL is valid but does not have a specified port, the function attaches a default port.
/// The default port is 443 if the URL uses the HTTPS scheme, and 80 otherwise.
///
/// # Parameters
///
/// - `url_str`: The URL string to parse.
///
/// # Returns
///
/// An `Option` which contains a String with the parsed URL if successful, or `None` if the input was not a valid URL.
pub fn get_url_with_port(url_str: &str) -> Option<String> {
    let url = Url::parse(url_str).ok()?;
    let default_port = url.scheme() == "https" && url.port().is_none();
    let port = url.port().unwrap_or_else(|| if default_port { 443 } else { 80 });
    Some(format!("{}://{}:{}{}", url.scheme(), url.host_str()?, port, url.path()))
}

/// Converts a string to a `U256` number.
///
/// The function takes a string as input and attempts to parse it as a decimal `U256` number.
/// If the parsing fails, it returns an error.
///
/// # Parameters
///
/// - `s`: The string to parse.
///
/// # Returns
///
/// A `Result` which is:
/// - Ok: Contains the parsed `U256` number.
/// - Err: Contains a string error message indicating that the parsing failed.
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
