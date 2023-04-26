// cast send subcommands
use crate::opts::{cast::parse_name_or_address, EthereumOpts, TransactionOpts};
use cast::TxBuilder;
use clap::Parser;
use ethers::{providers::Middleware, types::NameOrAddress};
use foundry_common::try_get_http_provider;
use foundry_config::{Chain, Config};

//for zksync
use zksync::types::{H160, H256};
use zksync::zksync_eth_signer::PrivateKeySigner;
use zksync::zksync_types::{L2ChainId, PackedEthSignature};
use zksync::{self, signer::Signer, wallet};

use ethers::types::{Address, U256};

use ethabi::{ParamType, Token};
use std::str::FromStr;

/// CLI arguments for `cast send`.
#[derive(Debug, Parser)]
pub struct ZkSendTxArgs {
    #[clap(
            help = "The destination of the transaction. If not provided, you must use cast zksend --create.",
             value_parser = parse_name_or_address,
            value_name = "TO"
        )]
    to: Option<NameOrAddress>,
    #[clap(help = "The signature of the function to call.", value_name = "SIG")]
    sig: Option<String>,
    #[clap(help = "The arguments of the function to call.", value_name = "ARGS")]
    args: Vec<String>,
    #[clap(
        long = "async",
        env = "CAST_ASYNC",
        name = "async",
        alias = "cast-async",
        help = "Only print the transaction hash and exit immediately."
    )]
    cast_async: bool,
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

    #[clap(subcommand)]
    command: Option<ZkSendTxSubcommands>,
}

#[derive(Debug, Parser)]
pub enum ZkSendTxSubcommands {
    #[clap(name = "--create", about = "Use to deploy raw contract bytecode")]
    Create {
        #[clap(help = "Bytecode of contract.", value_name = "CODE")]
        code: String,
        #[clap(help = "The signature of the function to call.", value_name = "SIG")]
        sig: Option<String>,
        #[clap(help = "The arguments of the function to call.", value_name = "ARGS")]
        args: Vec<String>,
    },
    #[clap(name = "--zksync", about = "send to zksync contract")]
    ZkSync {
        #[clap(help = "Chain Id. Local: 270, Testnet: 280.", value_name = "CHAIN-ID")]
        chain_id: u16,
    },
    #[clap(name = "--zksync-deposit", about = "Use for zksync L1 / L2 deposits")]
    ZkSyncDeposit {
        #[clap(help = "Chain Id. Local: 270, Testnet: 280.", value_name = "CHAIN-ID")]
        chain_id: u16,
        #[clap(help = "Deposit TO Address.", value_name = "TO")]
        to: String,
        #[clap(help = "Transfer amount.", value_name = "AMOUNT")]
        amount: i32,
        #[clap(help = "Transfer token. Leave blank for ETH.", value_name = "TOKEN")]
        token: Option<String>,
    },
    #[clap(name = "--zksync-withdraw", about = "Use for zksync L2 / L1 withdrawals")]
    ZkSyncWithdraw {
        #[clap(help = "Chain Id. Local: 270, Testnet: 280.", value_name = "CHAIN-ID")]
        chain_id: u16,
        #[clap(help = "Withdraw TO Address.", value_name = "TO")]
        to: String,
        #[clap(help = "Withdraw amount.", value_name = "AMOUNT")]
        amount: i32,
        #[clap(help = "Withdraw token. Leave blank for ETH.", value_name = "TOKEN")]
        token: Option<String>,
    },
}

impl ZkSendTxArgs {
    pub async fn run(self) -> eyre::Result<()> {
        println!("{:#?}, ZksendTxArgs", self);
        let config = Config::load();

        // get signer
        let signer = self.get_signer();
        let provider = try_get_http_provider(config.get_rpc_url_or_localhost_http()?)?;
        let chain: Chain = if let Some(chain) = self.eth.chain {
            chain
        } else {
            provider.get_chainid().await?.into()
        };
        let to_address = self.get_to_address();

        let sig = match self.sig {
            Some(sig) => sig,
            None => "".to_string(),
        };

        let params = if !sig.is_empty() { Some((&sig[..], self.args.clone())) } else { None };
        let mut builder = TxBuilder::new(&provider, config.sender, self.to, chain, true).await?;
        builder.args(params).await?;
        let (tx, func) = builder.build();
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
