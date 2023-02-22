use std::str::FromStr;
use serde_json;
use ethers::abi::ethabi;



pub fn load_contract(raw_abi_string: &str) -> ethabi::Contract {
    let abi_string = serde_json::Value::from_str(raw_abi_string)
        .expect("Malformed contract abi file")
        .get("abi")
        .expect("Malformed contract abi file")
        .to_string();
    ethabi::Contract::load(abi_string.as_bytes()).unwrap()
}