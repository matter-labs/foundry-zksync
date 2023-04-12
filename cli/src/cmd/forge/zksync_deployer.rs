use ethers::{
    abi::{encode, Token},
    prelude::Project,
    solc::info::ContractInfo,
    types::Bytes,
};
use foundry_config::Config;
use rustc_hex::ToHex;
use serde_json;
use std::{fs, io::Result};
use zksync::{
    self, signer,
    types::H256,
    wallet,
    zksync_eth_signer::PrivateKeySigner,
    zksync_types::{L2ChainId, PackedEthSignature},
};
use zksync_types::{Address, CONTRACT_DEPLOYER_ADDRESS};

pub struct ZkSyncDeployerOpts {}

pub struct ZkSyncDeployerBuilder {}

pub struct ZkSyncDeployer {
    
}

impl ZkSyncDeployer {
    pub fn get_bytecode() {}

    pub fn get_signer() {}

    pub fn encode_constructor_args() {}

    pub fn get_deployer_ctx() {}

    pub fn estimate_gas() {}

    pub fn deploy() {}
}
