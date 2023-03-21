use std::io::Result;
use zksync;
use zksync::types::{Address, H256};
use zksync::zksync_eth_signer::PrivateKeySigner;
use zksync::zksync_types::{L2ChainId, PackedEthSignature};
use zksync::{signer, wallet};
use zksync_types::L2_ETH_TOKEN_ADDRESS;

pub async fn transfer_zksync(
    to: &String,
    amount: &i32,
    token: &Option<String>,
    rpc: &Option<String>,
    p_key: &Option<String>,
    chain_id: u16,
    withdraw: bool,
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

    let private_key = H256::from_slice(&decode_hex(pk).unwrap());
    let eth_signer = PrivateKeySigner::new(private_key);
    let signer_addy = PackedEthSignature::address_from_private_key(&private_key)
        .expect("Can't get an address from the private key");
    let _signer = signer::Signer::new(eth_signer, signer_addy, L2ChainId(chain_id));
    println!("{:#?}, _signer ---->>>", _signer);

    let wallet = wallet::Wallet::with_http_client(rpc_str, _signer);
    let token_address;
    if !withdraw {
        token_address = match token {
            Some(token_addy) => Address::from_slice(&decode_hex(token_addy).unwrap()),
            None => L2_ETH_TOKEN_ADDRESS,
        };
    } else {
        token_address = match token {
            Some(token_addy) => Address::from_slice(&decode_hex(token_addy).unwrap()),
            None => Address::zero(),
        };
    }
    println!("{:#?}, token ---->>>", token);
    match &wallet {
        Ok(w) => {
            // Build Transfer //

            // let estimate_fee = w
            //     .start_transfer()
            //     .str_to(to)
            //     .unwrap()
            //     .amount(zksync_types::U256::from(amount.clone()))
            //     .token(token)
            //     .estimate_fee(None)
            //     .await
            //     .unwrap();
            // println!("{:#?}, <----------> estimate_fee", estimate_fee);

            let tx = w
                .start_transfer()
                .str_to(to)
                .unwrap()
                .amount(zksync_types::U256::from(amount.clone()))
                .token(token_address)
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

    // println!("{:#?}, wallet", wallet);
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
