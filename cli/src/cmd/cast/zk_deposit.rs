/// This module handles Bridging assets to ZkSync from Layer 1.
/// It defines the CLI arguments for the `cast zk-deposit` command and provides functionality
/// for depositing assets onto zkSync.
///
/// The module contains the following components:
/// - `ZkDepositTxArgs`: Struct representing the command line arguments for the `cast zk-deposit` command.
///     It includes parameters such as the destination address, amount to deposit, bridge address,
///     operator tip, zkSync RPC endpoint, and token to bridge.
/// - `ZkDepositTxArgs` implementation: Defines methods for executing the deposit transaction based on the provided
///     command line arguments.
/// - Helper functions:
///     - `get_url_with_port`: Parses a URL string and attaches a default port if one is not specified.
///     - `parse_decimal_u256`: Converts a string to a `U256` number.
///
use crate::{
    cmd::cast::zk_utils::zk_utils::{
        get_chain, get_private_key, get_rpc_url, get_signer, get_url_with_port,
    },
    opts::{cast::parse_name_or_address, TransactionOpts, Wallet},
};
use clap::Parser;
use ethers::types::NameOrAddress;
use foundry_config::Chain;
use zksync::{
    self,
    types::{Address, H160, U256},
    wallet,
};

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

    /// The zkSync RPC Layer 2 endpoint.
    /// Can be provided via the env var L2_RPC_URL
    /// or --l2-url from the command line.
    ///
    /// NOTE: For Deposits, L1_RPC_URL, or --l1-url should be set to the Layer 1 RPC URL
    #[clap(
        env = "L2_RPC_URL",
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
    /// We use the options directly, as we want to have a separate URL 
    #[clap(env = "L1_RPC_URL", long = "l1-rpc-url", help = "The L1 RPC endpoint.", value_name = "L1_URL")]
    pub l1_url: Option<String>,

    #[clap(long, env = "CHAIN", value_name = "CHAIN_NAME")]
    pub chain: Option<Chain>,

    #[clap(flatten)]
    pub wallet: Wallet,

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
        let private_key = get_private_key(&self.wallet.private_key)?;
        let l1_url = get_rpc_url(&self.l1_url)?;
        let l2_url = get_url_with_port(&self.l2_url).expect("Invalid L2_RPC_URL");
        let chain = get_chain(self.chain)?;
        let signer = get_signer(private_key, &chain);
        let wallet = wallet::Wallet::with_http_client(&l2_url, signer);
        let to_address = self.get_to_address();
        let token_address: Address = match self.token {
            Some(token_addy) => token_addy,
            None => Address::zero(),
        };

        match wallet {
            Ok(w) => {
                println!("Bridging assets....");
                let eth_provider = w.ethereum(l1_url).await.map_err(|e| e)?;
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
