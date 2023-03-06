use ethers::types::NameOrAddress;
use std::io::Result;
use zksync;
use zksync::types::H256;
use zksync::zksync_eth_signer::PrivateKeySigner;
use zksync::zksync_types::{L2ChainId, PackedEthSignature};
use zksync::{signer, wallet};
use zksync_types::L2_ETH_TOKEN_ADDRESS;

pub async fn transfer_zksync(
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

    let wallet = wallet::Wallet::with_http_client(rpc_str, _signer);
    match &wallet {
        Ok(w) => {
            // Build Transfer //
            let estimate_fee = w
                .start_transfer()
                .to(signer_addy)
                .amount(zksync_types::U256::from(1000000000))
                .token(L2_ETH_TOKEN_ADDRESS)
                .estimate_fee(None)
                .await
                .unwrap();
            println!("{:#?}, <----------> estimate_fee", estimate_fee);

            let tx = w
                .start_transfer()
                .to(signer_addy)
                .amount(zksync_types::U256::from(1000000000))
                .token(L2_ETH_TOKEN_ADDRESS)
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
    // let path = env::current_dir()?;
    // println!("The current directory is {}", path.display());
    // const KEY: &str = "KEY";
    // // Our test environment variable.
    // env::set_var(KEY, "123");
    // assert_eq!(get_env(KEY), "123");
    // assert_eq!(parse_env::<i32>(KEY), 123);

    // let zkconfig: zksync_config::ZkSyncConfig = zksync_config::ZkSyncConfig::from_env();
    // println!("{:#?}, <----------> zkconfig", zkconfig);

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
