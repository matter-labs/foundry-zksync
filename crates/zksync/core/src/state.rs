use revm::primitives::{Address as rAddress, U256 as rU256};

use zksync_types::{
    get_nonce_key,
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
};

use crate::convert::{ConvertAddress, ConvertH160, ConvertH256, ConvertRU256, ConvertU256};

/// Returns balance storage slot
pub fn get_balance_storage(address: rAddress) -> (rAddress, rU256) {
    let balance_key = storage_key_for_eth_balance(&address.to_h160());
    let account = balance_key.address().to_address();
    let slot = balance_key.key().to_ru256();
    (account, slot)
}

/// Returns nonce storage slot
pub fn get_nonce_storage(address: rAddress) -> (rAddress, rU256) {
    let nonce_key = get_nonce_key(&address.to_h160());
    let account = nonce_key.address().to_address();
    let slot = nonce_key.key().to_ru256();
    (account, slot)
}

/// Returns full nonce value
pub fn new_full_nonce(tx_nonce: u64, deploy_nonce: u64) -> rU256 {
    nonces_to_full_nonce(tx_nonce.into(), deploy_nonce.into()).to_ru256()
}

/// Represents a ZKSync account nonce with two 64-bit transaction and deployment nonces.
#[derive(Default, Debug, Clone, Copy)]
pub struct FullNonce {
    /// Transaction nonce.
    pub tx_nonce: u64,
    /// Deployment nonce.
    pub deploy_nonce: u64,
}

/// Decomposes a full nonce into transaction and deploy nonces.
pub fn parse_full_nonce(full_nonce: rU256) -> FullNonce {
    let (tx, deploy) = decompose_full_nonce(full_nonce.to_u256());
    FullNonce { tx_nonce: tx.as_u64(), deploy_nonce: deploy.as_u64() }
}
