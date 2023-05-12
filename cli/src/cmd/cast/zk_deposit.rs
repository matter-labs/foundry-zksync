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

/// CLI arguments for `cast zk-deposit`.
#[derive(Debug, Parser)]
pub struct ZkDepositTxArgs {
    #[clap(
            help = "The destination of the transaction.",
             value_parser = parse_name_or_address,
            value_name = "TO"
        )]
    to: NameOrAddress,

    #[clap(
        long,
        short,
        help = "Amount of token to deposit.",
        value_name = "AMOUNT",
        value_parser = parse_decimal_u256
    )]
    amount: U256,

    #[clap(help = "The address of a custom bridge to call.", value_name = "BRIDGE")]
    bridge_address: Option<Address>,

    #[clap(
        help = "Optional fee that the user can choose to pay in addition to the regular transaction fee.",
        value_name = "TIP"
    )]
    operator_tip: Option<U256>,

    #[clap(
        env = "ZKSYNC_RPC_URL",
        long,
        short = 'z',
        help = "The zkSync RPC endpoint.",
        value_name = "L2URL"
    )]
    l2_url: String,

    #[clap(long, help = "Token to bridge. Leave blank for ETH.", value_name = "TOKEN")]
    token: Option<Address>,

    #[clap(flatten)]
    tx: TransactionOpts,

    #[clap(flatten)]
    eth: EthereumOpts,
}

impl ZkDepositTxArgs {
    pub async fn run(self) -> eyre::Result<()> {
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

        let rpc_url = self
            .eth
            .rpc_url()
            .expect("RPC URL was not provided. \nTry using --rpc-url flag or environment variable 'ETH_RPC_URL= '");

        let l2_url = get_url_with_port(&self.l2_url).expect("Invalid L2_RPC_URL");

        let chain = self
            .eth
            .chain
            .expect("Chain was not provided. \nTry using --chain flag (ex. --chain 270 ) \nor environment variable 'CHAIN= ' (ex.'CHAIN=270')");

        let signer = Self::get_signer(private_key, &chain);

        // getting port error retrieving this wallet, if no port provided
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

    fn get_signer(private_key: H256, chain: &Chain) -> Signer<PrivateKeySigner> {
        let eth_signer = PrivateKeySigner::new(private_key);
        let signer_addy = PackedEthSignature::address_from_private_key(&private_key)
            .expect("Can't get an address from the private key");
        Signer::new(eth_signer, signer_addy, L2ChainId(chain.id().try_into().unwrap()))
    }

    fn get_to_address(&self) -> H160 {
        let to = self.to.as_address().expect("Please enter TO address.");
        let deployed_contract = to.as_bytes();
        zksync_utils::be_bytes_to_safe_address(&deployed_contract).unwrap()
    }
}

/// This function includes a default port to
/// be compatible with jsonrpsee wallet
pub fn get_url_with_port(url_str: &str) -> Option<String> {
    let url = Url::parse(url_str).ok()?;
    let default_port = url.scheme() == "https" && url.port().is_none();
    let port = url.port().unwrap_or_else(|| if default_port { 443 } else { 80 });
    Some(format!("{}://{}:{}{}", url.scheme(), url.host_str()?, port, url.path()))
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
