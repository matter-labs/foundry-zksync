use revm::primitives::{Address as rAddress, U256 as rU256};
use zksync_types::{get_nonce_key, utils::storage_key_for_eth_balance};

use crate::convert::{ConvertAddress, ConvertH160, ConvertH256};

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
