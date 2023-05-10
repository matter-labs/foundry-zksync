// cast send subcommands
use crate::opts::{cast::parse_name_or_address, EthereumOpts, TransactionOpts};
use cast::{Cast, TxBuilder};
use clap::Parser;
use ethers::types::NameOrAddress;
use foundry_config::{Chain, Config};
use std::str::FromStr;

use zksync::ethereum::{l1_bridge_contract, zksync_contract};
use zksync::types::H256;
use zksync::zksync_types::{L2ChainId, PackedEthSignature};
// use zksync::{self, signer::Signer, wallet};
use zksync_eth_signer::PrivateKeySigner;
use zksync_types::L2_ETH_TOKEN_ADDRESS;
use zksync_types::REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE;

use ethers::prelude::*;
use ethers::signers::{LocalWallet, Signer, Wallet};

pub const ZKSYNC_DEFAULT_L1_ERC20_BRIDGE: &str = "0x927ddfcc55164a59e0f33918d13a2d559bc10ce7";
pub const ZKSYNC_MAINCONTRACT_TESTNET: &str = "0x1908e2bf4a88f91e4ef0dc72f02b8ea36bea2319";

// pub const ZKSYNC_MAINCONTRACT_MAINNET: u64 = 0x32400084c286cf3e17e7b677ea9583e60a000324;
/// CLI arguments for `cast zk-send`.
#[derive(Debug, Parser)]
pub struct ZkDepositTxArgs {
    #[clap(
            help = "The destination of the transaction.",
             value_parser = parse_name_or_address,
            value_name = "TO"
        )]
    to: NameOrAddress,

    #[clap(help = "The address of a custom bridge to call.", value_name = "BRIDGE")]
    bridge_address: Option<Address>,

    #[clap(
        help = "Optional fee that the user can choose to pay in addition to the regular transaction fee.",
        value_name = "TIP"
    )]
    operator_tip: Option<U256>,

    #[clap(
        env = "ZKSYNC_RPC_URL",
        long = "l2-rpc-url",
        help = "The zkSync RPC endpoint.",
        value_name = "L2URL"
    )]
    l2_rpc_url: String,

    #[clap(long, help = "Token to bridge. Leave blank for ETH.", value_name = "TOKEN")]
    token: Option<Address>,

    #[clap(flatten)]
    tx: TransactionOpts,

    #[clap(flatten)]
    eth: EthereumOpts,
}

impl ZkDepositTxArgs {
    pub async fn run(self) -> eyre::Result<()> {
        println!("{:#?}, ZkDepositTxArgs", self);

        //get private key
        let private_key = match &self.eth.wallet.private_key {
            Some(pkey) => {
                let decoded = match decode_hex(pkey) {
                    Ok(val) => H256::from_slice(&val),
                    Err(e) => {
                        panic!("Error parsing private key {e}, make sure it is valid.")
                    }
                };
                decoded
            }
            None => {
                panic!("Private key was not provided. Try using --private-key flag");
            }
        };

        //verify rpc url has been populated
        if let None = &self.eth.rpc_url {
            panic!("RPC URL was not provided. Try using --rpc-url flag or environment variable 'ETH_RPC_URL= '");
        }

        //get chain
        let chain = match self.eth.chain {
            Some(chain) => chain,
            None => {
                panic!("Chain was not provided. Use --chain flag (ex. --chain 270 ) or environment variable 'CHAIN= ' (ex.'CHAIN=270')");
            }
        };

        //get to address
        let to = self.to.as_address().expect("Please enter TO address.").clone();
        let encoded_to = encode_hex(to.as_bytes());

        let token_address: Address = match self.token {
            Some(token_addy) => token_addy,
            None => Address::zero(),
        };

        //get amount
        let amount = match self.tx.value {
            Some(amt) => amt,
            None => {
                panic!("Amount was not provided. Use --value flag (ex. --amount 1000000000 )")
            }
        };

        let provider = Provider::<Http>::try_from(self.eth.rpc_url.as_ref().unwrap())?;
        let block_number: U64 = provider.get_block_number().await?;
        // println!("{block_number}");

        let is_eth_deposit = token_address == Address::zero();

        // using default for eth
        let gas_limit = 200_000u64;
        //get gas price
        let gas_price = provider.get_gas_price().await?;
        println!("{gas_price}, gas price");

        let main_contract_address: ethers::types::H160 =
            ethers::types::H160::from_str("0x1908e2bf4a88f91e4ef0dc72f02b8ea36bea2319").unwrap();

        let l2_gas_limit = ethers::types::U256::from(3_000_000u32);
        let from = self.eth.wallet.from.unwrap_or(ethers::types::H160::zero());

        let maincontract = zksync_contract();
        let base_cost_function =
            maincontract.functions_by_name("l2TransactionBaseCost").unwrap().get(0).unwrap();

        let bridge_contract = l1_bridge_contract();

        let mut builder =
            TxBuilder::new(&provider, from, Some(main_contract_address), chain, false).await?;
        builder
            // .gas(Some(gas_limit.into()))
            // .etherscan_api_key(config.get_etherscan_api_key(Some(chain)))
            .gas_price(Some(gas_price))
            .priority_gas_price(self.tx.priority_gas_price)
            .nonce(self.tx.nonce);

        builder
            .set_args(
                // "l2TransactionBaseCost(uint256,uint256,uint256)(uint256)",
                &base_cost_function.signature(),
                vec![gas_price.to_string(), l2_gas_limit.to_string(), 800.to_string()],
            )
            .await?;

        let (tx, func) = builder.build();
        let call_result = provider.call(&tx, None).await?;
        let base_cost = U256::from_big_endian(&call_result);

        // Calculate the amount of ether to be sent in the transaction.
        let total_value = if is_eth_deposit {
            base_cost + self.operator_tip.unwrap_or_else(|| U256::from(0)) + amount
        } else {
            base_cost + self.operator_tip.unwrap_or_else(|| U256::from(0))
        };

        let maincontract = zksync_contract();
        let request_l2_tx_function =
            maincontract.functions_by_name("requestL2Transaction").unwrap().get(0).unwrap();

        let calldata: Bytes = Default::default();
        let factory_deps: &[u8] = &Vec::new();
        println!("calldata: {:#?}", calldata);
        println!("factory_deps: {:#?}", factory_deps);
        println!("bridge_contract: {:#?}", bridge_contract);

        let mut builder =
            TxBuilder::new(&provider, from, Some(main_contract_address), chain, false).await?;
        builder
            .gas(Some(gas_limit.into()))
            .value(Some(total_value))
            .gas_price(Some(gas_price))
            .priority_gas_price(self.tx.priority_gas_price)
            .nonce(self.tx.nonce);

        builder
            .set_args(
                &request_l2_tx_function.signature(),
                vec![
                    encoded_to.to_owned(),
                    amount.to_string(),
                    calldata.to_string(),
                    l2_gas_limit.to_string(),
                    800.to_string(),
                    "[]".to_string(),
                    encoded_to,
                ],
            )
            .await?;
        let builder_output = builder.build();

        let wlt = self.eth.wallet.private_key().unwrap().unwrap();
        println!("wlt: {:#?}", wlt);

        let wallet: LocalWallet = self.eth.wallet.private_key.unwrap().parse::<LocalWallet>()?;
        println!("wallet: {:#?}", wallet);
        // let siner = wlt.chain(5);

        // provider.

        let cast = Cast::new(provider);

        let pending_tx = cast.send(builder_output).await?;
        let tx_hash = *pending_tx;
        //-----------------------------------//
        //-----------------------------------//
        //-----------------------------------//

        // // get signer
        // let signer = Self::get_signer(private_key, &chain);
        // // let provider = try_get_http_provider(config.get_rpc_url_or_localhost_http()?)?;
        // let to_address = self.to.as_address().expect("error getting to address").clone();

        // let wallet = wallet::Wallet::with_http_client(&self.eth.rpc_url.unwrap(), signer);

        // // IF BRIDGING

        // match &wallet {
        //     Ok(w) => {
        //         println!("Bridging assets....");

        //         // Build Transfer //
        //         // let tx = w
        //         //     .start_transfer()
        //         //     .to(to_address)
        //         //     .amount(amount)
        //         //     .token(token_address)
        //         //     .send()
        //         //     .await
        //         //     .unwrap();
        //         // let tx_rcpt_commit = tx.wait_for_commit().await.unwrap();
        //         // println!("Transaction Hash: {:#?}", tx_rcpt_commit.transaction_hash);
        //     }
        //     Err(e) => panic!("error wallet: {e:?}"),
        // };

        Ok(())
    }

    // fn get_signer(private_key: H256, chain: &Chain) -> Signer<PrivateKeySigner> {
    //     let eth_signer = PrivateKeySigner::new(private_key);
    //     let signer_addy = PackedEthSignature::address_from_private_key(&private_key)
    //         .expect("Can't get an address from the private key");
    //     Signer::new(eth_signer, signer_addy, L2ChainId(chain.id().try_into().unwrap()))
    // }
}

fn parse_decimal_u256(s: &str) -> Result<U256, String> {
    match U256::from_dec_str(s) {
        Ok(value) => Ok(value),
        Err(e) => Err(format!("Failed to parse decimal number: {}", e)),
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
