use ethers::abi::{encode, Token};
use ethers::types::NameOrAddress;
use sha2::Digest;
use std::env;
use std::io::Result;
use zksync;
use zksync::types::H256;
use zksync::zksync_eth_signer::PrivateKeySigner;
use zksync::zksync_types::{L2ChainId, PackedEthSignature};
use zksync::{signer, wallet};
use zksync_config;
use zksync_types::zk_evm::sha3::Keccak256;
use zksync_utils::{get_env, parse_env};

pub async fn send_zksync(
    to: &Option<NameOrAddress>,
    args: &Vec<String>,
    sig: &Option<String>,
    rpc: &Option<String>,
    p_key: &Option<String>,
) -> Result<()> {
    //rpc url
    let mut rpc_str: &str = "";
    if let Some(val) = rpc {
        rpc_str = val;
    }
    //private key
    let mut pk: &str = "";
    if let Some(val) = p_key {
        pk = val;
    }

    // let pk = "d5b54c3da4bd2722bb9dd3df5aa86e71b8db43560be88b1a271feb4df3268b54";
    let private_key = H256::from_slice(&decode_hex(pk).unwrap());
    let eth_signer = PrivateKeySigner::new(private_key);
    let signer_addy = PackedEthSignature::address_from_private_key(&private_key)
        .expect("Can't get an address from the private key");
    let _signer = signer::Signer::new(eth_signer, signer_addy, L2ChainId(280));
    println!("{:#?}, _signer ---->>>", _signer);

    //SUCCESSFULLY DEPLOYED AND MANUALLY VERIFIED GREETER CONTRACT TO ZKSYNC
    // 0x8059F965610FaD505E4e51c7b1462cBc7049ed10

    let deployed_contract = to.as_ref().unwrap().as_address().unwrap().clone();
    let deployed_contract = deployed_contract.as_bytes();
    let deployed_contract = zksync_utils::be_bytes_to_safe_address(&deployed_contract).unwrap();
    let function_signature: &str = &sig.as_ref().unwrap();

    let mut arg_tokens: Vec<Token> = Vec::new();
    for arg in args {
        arg_tokens.push(Token::String(arg.clone()));
    }

    let mut signed = [0u8; 4];
    let digest = &Keccak256::digest(function_signature)[..signed.len()];
    signed.copy_from_slice(digest);

    let encoded = encode(&arg_tokens);
    let encoded_function_call: Vec<u8> = signed.into_iter().chain(encoded.into_iter()).collect();
    // println!("{:#?}, encoded_function_call", encoded_function_call);

    let wallet = wallet::Wallet::with_http_client(rpc_str, _signer);
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
                .contract_address(deployed_contract)
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

    println!("<---- IGNORE ERRORS BELOW THIS LINE---->>>");

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
