// cast send subcommands
use crate::opts::{cast::parse_name_or_address, EthereumOpts, TransactionOpts, WalletType};
use cast::{Cast, TxBuilder};
use clap::Parser;
use ethers::{abi::encode, providers::Middleware, types::NameOrAddress};
use foundry_common::try_get_http_provider;
use foundry_config::{Chain, Config};
use std::sync::Arc;

//for zksync
use crate::cmd::cast::{send_zksync, transfer_zksync};
use sha2::Digest;
use zksync::types::{H160, H256};
use zksync::zksync_eth_signer::PrivateKeySigner;
use zksync::zksync_types::{L2ChainId, PackedEthSignature};
use zksync::{self, signer::Signer, wallet};
use zksync_types::zk_evm::sha3::Keccak256;

use ethers::types::U256;

use ethers::abi::token::Tokenizer;

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
        // println!("{:#?}, ZksendTxArgs", self);
        let mut config = Config::load();
        // println!("{:#?}, config", config);

        // get signer
        let signer = self.get_signer();
        let to_address = self.get_to_address();
        let function_signature: &str = &self.sig.as_ref().unwrap();

        println!("{:#?}, self.args", self.args);

        let mut arg_tokens: Vec<Token> = Vec::new();
        for arg in &self.args {
            arg_tokens.push(Token::String(arg.clone()));
        }

        let mut signed = [0u8; 4];
        let hashed_sig = &Keccak256::digest(function_signature)[..signed.len()];
        signed.copy_from_slice(hashed_sig);

        let encoded = encode(&arg_tokens);
        let encoded_function_call: Vec<u8> =
            signed.into_iter().chain(encoded.into_iter()).collect();
        // println!("{:#?}, encoded_function_call", encoded_function_call);

        // let provider = Provider::<Http>::try_from(
        //     "https://mainnet.infura.io/v3/c60b0bb42f8a4c6481ecd229eddaca27",
        // )
        // .expect("could not instantiate HTTP Provider");

        let provider = try_get_http_provider(config.get_rpc_url_or_localhost_http()?)?;
        let chain: Chain = if let Some(chain) = self.eth.chain {
            chain
        } else {
            provider.get_chainid().await?.into()
        };
        println!("{:#?}, provider", provider);
        println!("{:#?}, chain", chain);

        let sig = match self.sig {
            Some(sig) => sig,
            None => "".to_string(),
        };

        let params = if !sig.is_empty() { Some((&sig[..], self.args.clone())) } else { None };
        let mut builder = TxBuilder::new(&provider, config.sender, self.to, chain, true).await?;

        println!("{:#?}, params", params);
        builder.args(params).await?;
        let (tx, func) = builder.build();

        // Define the function signature and input types

        let function = func.unwrap();
        let arguments = self.args.clone();
        let input_types = function.inputs.clone();
        // Define the input parameter types as a Vec<ParamType>
        let input_param_types =
            input_types.iter().map(|param| param.kind.clone()).collect::<Vec<ParamType>>();

        println!("{:#?}, input_param_types", input_param_types);

        let tokens = convert_args_to_tokens(arguments.as_slice(), &input_param_types).unwrap();
        println!("Tokens: {:?}", tokens);

        // // Encode the input parameters as a byte array
        let encoded_input = function.encode_input(tokens.as_slice()).unwrap();

        // Calculate the function selector
        // let selector = keccak256(function.signature().as_bytes()).as_fixed_bytes();

        // Combine the selector and the encoded input arguments into a single Vec<u8>
        // let encoded = [selector, tokens].concat();

        println!("{:?}, encoded_input", encoded_input);

        // Encode the function call data using the input tokens
        // let encoded_data = function.encode_input(&tokens.unwrap().as_slice()).unwrap();

        // println!("{:#?}, tx", tx);
        // println!("{:#?}, inputs", inputs);
        println!("{:#?}, function", function);

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

#[allow(clippy::too_many_arguments)]
async fn cast_send<M: Middleware, F: Into<NameOrAddress>, T: Into<NameOrAddress>>(
    provider: M,
    from: F,
    to: Option<T>,
    code: Option<String>,
    args: (String, Vec<String>),
    tx: TransactionOpts,
    chain: Chain,
    etherscan_api_key: Option<String>,
    cast_async: bool,
    confs: usize,
    to_json: bool,
) -> eyre::Result<()>
where
    M::Error: 'static,
{
    let (sig, params) = args;
    let params = if !sig.is_empty() { Some((&sig[..], params)) } else { None };
    let mut builder = TxBuilder::new(&provider, from, to, chain, tx.legacy).await?;
    builder
        .etherscan_api_key(etherscan_api_key)
        .gas(tx.gas_limit)
        .gas_price(tx.gas_price)
        .priority_gas_price(tx.priority_gas_price)
        .value(tx.value)
        .nonce(tx.nonce);

    if let Some(code) = code {
        let mut data = hex::decode(code.strip_prefix("0x").unwrap_or(&code))?;

        if let Some((sig, args)) = params {
            let (mut sigdata, _) = builder.create_args(sig, args).await?;
            data.append(&mut sigdata);
        }

        builder.set_data(data);
    } else {
        builder.args(params).await?;
    };
    let builder_output = builder.build();

    let cast = Cast::new(provider);

    let pending_tx = cast.send(builder_output).await?;
    let tx_hash = *pending_tx;

    if cast_async {
        println!("{tx_hash:#x}");
    } else {
        let receipt = cast.receipt(format!("{tx_hash:#x}"), None, confs, false, to_json).await?;
        println!("{receipt}");
    }

    Ok(())
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

use ethabi::{ParamType, Token};

fn convert_args_to_tokens(
    args: &[String],
    input_types: &[ParamType],
) -> Result<Vec<Token>, String> {
    if args.len() != input_types.len() {
        return Err("The number of arguments does not match the number of input types".to_owned());
    }

    let mut tokens = Vec::with_capacity(args.len());
    for (i, arg) in args.iter().enumerate() {
        let token = match &input_types[i] {
            ParamType::String => Token::String(arg.clone()),
            ParamType::Uint(_) => Token::Uint(
                U256::from_dec_str(arg)
                    .map_err(|_| format!("Failed to parse argument at index {}", i))?,
            ),
            // Add more cases for other parameter types if needed
            _ => return Err(format!("Unsupported input type: {:?}", input_types[i])),
        };
        tokens.push(token);
    }

    Ok(tokens)
}
