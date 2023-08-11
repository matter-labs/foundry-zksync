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
    cmd::cast::zk_utils::zk_utils::{get_chain, get_private_key, get_rpc_url, get_url_with_port},
    opts::{cast::parse_name_or_address, TransactionOpts, Wallet},
};
use clap::Parser;
use ethers::types::NameOrAddress;
use foundry_config::Chain;
use std::str::FromStr;
use zksync_web3_rs::providers::Provider;
use zksync_web3_rs::signers::{LocalWallet, Signer};
use zksync_web3_rs::types::{Address, H160, U256};
use zksync_web3_rs::DepositRequest;
use zksync_web3_rs::ZKSWallet;

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

    /// Layer 2 gas limit.
    #[clap(help = "Layer 2 gas limit", value_name = "L2_GAS_LIMIT")]
    l2_gas_limit: Option<U256>,

    /// Set the gas per pubdata byte (Optional).
    #[clap(help = "Set the gas per pubdata byte (Optional)", value_name = "GAS_PER_PUBDATA_BYTE")]
    gas_per_pubdata_byte: Option<U256>,

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
    #[clap(
        env = "L1_RPC_URL",
        long = "l1-rpc-url",
        help = "The L1 RPC endpoint.",
        value_name = "L1_URL"
    )]
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
        let chain: Chain = get_chain(self.chain)?;
        let l1_provider = Provider::try_from(l1_url)?;
        let l2_provider = Provider::try_from(l2_url)?;
        let wallet = LocalWallet::from_str(&format!("{private_key:?}"))?.with_chain_id(chain);
        let zk_wallet =
            ZKSWallet::new(wallet, None, Some(l2_provider.clone()), Some(l1_provider.clone()))
                .unwrap();

        // TODO Support different tokens than ETH.
        let deposit_request = DepositRequest::new(self.amount.into())
            .to(self.get_to_address())
            .operator_tip(self.operator_tip.unwrap_or(0.into()))
            .gas_price(self.tx.gas_price)
            .gas_limit(self.tx.gas_limit)
            .gas_per_pubdata_byte(self.gas_per_pubdata_byte)
            .l2_gas_limit(self.l2_gas_limit);

        println!("Bridging assets....");
        let l1_receipt = zk_wallet.deposit(&deposit_request).await.unwrap();
        println!("Transaction Hash: {:#?}", l1_receipt.transaction_hash);

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
        Address::from_slice(deployed_contract)
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

#[cfg(test)]
mod zk_deposit_tests {
    use std::env;

    use super::*;

    #[tokio::test]
    async fn test_deposit_to_signer_account() {
        let amount = U256::from(1);
        let private_key = "0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110";
        let l1_url = env::var("L1_RPC_URL").ok();
        let l2_url = env::var("L2_RPC_URL").unwrap();

        let zk_wallet = {
            let l1_provider = Provider::try_from(l1_url.unwrap()).unwrap();
            let l2_provider = Provider::try_from(l2_url.clone()).unwrap();

            let wallet = LocalWallet::from_str(private_key).unwrap();
            let zk_wallet =
                ZKSWallet::new(wallet, None, Some(l2_provider), Some(l1_provider)).unwrap();

            zk_wallet
        };

        let l1_balance_before = zk_wallet.eth_balance().await.unwrap();
        let l2_balance_before = zk_wallet.era_balance().await.unwrap();

        let zk_deposit_tx_args = {
            let to = parse_name_or_address("0x36615Cf349d7F6344891B1e7CA7C72883F5dc049").unwrap();
            let bridge_address = None;
            let operator_tip = None;
            let token = None; // => Ether.
            let tx = TransactionOpts {
                gas_limit: None,
                gas_price: None,
                priority_gas_price: None,
                value: None,
                nonce: None,
                legacy: false,
            };
            let l1_url = env::var("L1_RPC_URL").ok();
            let chain = Some(Chain::Id(env::var("CHAIN").unwrap().parse().unwrap()));
            let wallet: Wallet = Wallet::parse_from(["foundry-cli", "--private-key", private_key]);

            ZkDepositTxArgs {
                to,
                amount,
                bridge_address,
                operator_tip,
                l2_url,
                token,
                tx,
                l1_url,
                chain,
                wallet,
                l2_gas_limit: None,
                gas_per_pubdata_byte: None,
            }
        };

        zk_deposit_tx_args.run().await.unwrap();

        let l1_balance_after = zk_wallet.eth_balance().await.unwrap();
        let l2_balance_after = zk_wallet.era_balance().await.unwrap();
        println!("L1 balance after: {}", l1_balance_after);
        println!("L2 balance after: {}", l2_balance_after);

        assert!(
            l1_balance_after <= l1_balance_before - amount,
            "Balance on L1 should be decreased"
        );
        assert!(
            l2_balance_after >= l2_balance_before + amount,
            "Balance on L2 should be increased"
        );
    }
}
