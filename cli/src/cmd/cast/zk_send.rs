// cast send subcommands
use crate::opts::{cast::parse_name_or_address, EthereumOpts, TransactionOpts};
use cast::TxBuilder;
use clap::Parser;
use ethers::types::NameOrAddress;
use foundry_common::try_get_http_provider;
use foundry_config::{Chain, Config};
use zksync::types::{Address, H160, H256, U256};
use zksync_types::L2_ETH_TOKEN_ADDRESS;

use zksync::zksync_types::{L2ChainId, PackedEthSignature};
use zksync::{self, signer::Signer, wallet};
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
    pub async fn run(self) -> eyre::Result<()> {
        let config = Config::load();
        //get private key
        let private_key = self
            .eth
            .wallet
            .private_key
            .as_ref()
            .and_then(|pkey| {
                decode_hex(pkey)
                    .map_err(|e| format!("Error parsing private key: {}", e))
                    .map(|val| H256::from_slice(&val))
                    .ok()
            })
            .expect("Private key was not provided. Try using --private-key flag");

        //verify rpc url has been populated
        if self.eth.rpc_url.is_none() {
            eyre::bail!("RPC URL was not provided. Try using --rpc-url flag or environment variable 'ETH_RPC_URL= '");
        }

        //get chain
        let chain = self.eth.chain
            .expect("Chain was not provided. Use --chain flag (ex. --chain 270 ) or environment variable 'CHAIN= ' (ex.'CHAIN=270')");

        // get signer
        let signer = Self::get_signer(private_key, &chain);
        let provider = try_get_http_provider(config.get_rpc_url_or_localhost_http()?)?;
        let to_address = self.get_to_address();
        let sender = self.eth.sender().await;

        let wallet = wallet::Wallet::with_http_client(&self.eth.rpc_url.unwrap(), signer);
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
                    println!("Transaction Hash: {:#?}", rcpt.transaction_hash);
                }
                Err(e) => eyre::bail!("error wallet: {e:?}"),
            };
        } else {
            match wallet {
                Ok(w) => {
                    println!("Sending transaction....");

                    let sig = self.sig.expect("Error: Function Signature is empty");
                    let params = if !sig.is_empty() { Some((&sig[..], self.args)) } else { None };

                    let mut builder =
                        TxBuilder::new(&provider, sender, self.to, chain, true).await?;

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

                    let gas_used = rcpt.gas_used.expect("Error retrieving gas used");
                    let gas_price = rcpt.effective_gas_price.expect("Error retrieving gas price");
                    let block_number = rcpt.block_number.expect("Error retrieving block number");

                    println!("+-------------------------------------------------+");
                    println!("Transaction Hash: {:#?}", rcpt.transaction_hash);
                    println!("Gas used: {:#?}", gas_used);
                    println!("Effective gas price: {:#?}", gas_price);
                    println!("Block Number: {:#?}", block_number);
                    println!("+-------------------------------------------------+");

                    for log in rcpt.logs {
                        if log.address == CONTRACT_DEPLOYER_ADDRESS {
                            let deployed_address = log.topics.get(3).unwrap();
                            let deployed_address = Address::from(*deployed_address);
                            println!("Deployed contract address: {:#?}", deployed_address);
                        }
                    }

                    println!("+-------------------------------------------------+");
                }
                Err(e) => eyre::bail!("error wallet: {e:?}"),
            };
        }

        Ok(())
    }

    fn get_signer(private_key: H256, chain: &Chain) -> Signer<PrivateKeySigner> {
        let eth_signer = PrivateKeySigner::new(private_key);
        let signer_addy = PackedEthSignature::address_from_private_key(&private_key)
            .expect("Can't get an address from the private key");
        Signer::new(eth_signer, signer_addy, L2ChainId(chain.id().try_into().unwrap()))
    }

    fn get_to_address(&self) -> H160 {
        let to = self.to.as_ref().expect("Enter TO: Address");
        let deployed_contract = to.as_address().expect("Invalid address").as_bytes();
        zksync_utils::be_bytes_to_safe_address(&deployed_contract).unwrap()
    }
}

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
