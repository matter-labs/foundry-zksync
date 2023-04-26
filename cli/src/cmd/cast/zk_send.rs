// cast send subcommands
use crate::opts::{cast::parse_name_or_address, EthereumOpts, TransactionOpts};
use cast::TxBuilder;
use clap::Parser;
use ethers::{providers::Middleware, types::NameOrAddress};
use foundry_common::try_get_http_provider;
use foundry_config::{Chain, Config};
use zksync::types::{Address, H160, H256};
use zksync_types::L2_ETH_TOKEN_ADDRESS;

use crate::cmd::cast::{send_zksync, transfer_zksync};
use zksync::zksync_eth_signer::PrivateKeySigner;
use zksync::zksync_types::{L2ChainId, PackedEthSignature};
use zksync::{self, signer::Signer, wallet};

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
        help = "For L1 -> L2 deposits.",
        conflicts_with = "withdraw"
    )]
    deposit: bool,

    #[clap(
        long,
        short,
        help_heading = "Bridging options",
        help = "For L2 -> L1 withdrawals.",
        conflicts_with = "deposit"
    )]
    withdraw: bool,

    #[clap(
        long,
        help_heading = "Bridging options",
        help = "Token to bridge. Leave blank for ETH."
    )]
    token: Option<String>,

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
        println!("{:#?}, ZksendTxArgs", self);
        let config = Config::load();

        //get chain
        let chain = match self.eth.chain {
            Some(chain) => chain,
            None => {
                panic!("Chain was not provided. Use --chain flag (ex. --chain 270 ) or environment variable 'CHAIN' (ex.'CHAIN=270')");
            }
        };

        // get signer
        let signer = self.get_signer(&chain);
        let provider = try_get_http_provider(config.get_rpc_url_or_localhost_http()?)?;
        let to_address = self.get_to_address();

        // IF BRIDGING
        let token_address;
        if self.deposit {
            token_address = match &self.token {
                Some(token_addy) => Address::from_slice(&decode_hex(token_addy).unwrap()),
                None => L2_ETH_TOKEN_ADDRESS,
            };
        } else if self.withdraw {
            token_address = match &self.token {
                Some(token_addy) => Address::from_slice(&decode_hex(token_addy).unwrap()),
                None => Address::zero(),
            };
        }
        println!("{:#?}, self.token ---->>>", self.token);

        let sig = match self.sig {
            Some(sig) => sig,
            None => "".to_string(),
        };

        let params = if !sig.is_empty() { Some((&sig[..], self.args.clone())) } else { None };
        let mut builder = TxBuilder::new(&provider, config.sender, self.to, chain, true).await?;
        builder.args(params).await?;
        let (tx, _func) = builder.build();
        let encoded_function_call = tx.data().unwrap().to_vec();

        let wallet = wallet::Wallet::with_http_client(&self.eth.rpc_url.unwrap(), signer);
        match &wallet {
            Ok(w) => {
                // Build Executor //
                // let estimate_fee = w
                //     .start_execute_contract()
                //     .contract_address(deployed_contract)
                //     .calldata(encoded_function_call)
                //     .estimate_fee(None)
                //     .await
                //     .unwrap();
                // println!("{:#?}, <----------> estimate_fee", estimate_fee);

                let tx = w
                    .start_execute_contract()
                    .contract_address(to_address)
                    .calldata(encoded_function_call)
                    .send()
                    .await
                    .unwrap();
                println!("{:#?}, <----------> tx", tx);
                let tx_rcpt_commit = tx.wait_for_commit().await.unwrap();
                println!("{:#?}, <----------> tx_rcpt_commit", tx_rcpt_commit);
                // let tx_rcpt_finalize = tx.wait_for_finalize().await.unwrap();
                // println!("{:#?}, <----------> tx_rcpt_finalize", tx_rcpt_finalize);
            }
            Err(e) => panic!("error wallet: {e:?}"),
        };

        Ok(())
    }

    fn get_signer(&self, chain: &Chain) -> Signer<PrivateKeySigner> {
        // get signer
        let private_key =
            H256::from_slice(&decode_hex(&self.eth.wallet.private_key.clone().unwrap()).unwrap());
        let eth_signer = PrivateKeySigner::new(private_key);
        let signer_addy = PackedEthSignature::address_from_private_key(&private_key)
            .expect("Can't get an address from the private key");
        Signer::new(eth_signer, signer_addy, L2ChainId(chain.id().try_into().unwrap()))
    }
    fn get_to_address(&self) -> H160 {
        let deployed_contract = match &self.to {
            Some(to) => match to.as_address() {
                Some(addy) => addy.as_bytes(),
                None => panic!("Invalid Address"),
            },
            None => panic!("Enter TO: Address"),
        };
        zksync_utils::be_bytes_to_safe_address(&deployed_contract).unwrap()
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
