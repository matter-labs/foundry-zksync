use ethers::signers::LocalWallet;
use revm::primitives::{ruint::Uint, Address};
use zksync_basic_types::{H160, H256, U256};
use zksync_types::zkevm_test_harness::k256::{elliptic_curve::Curve, Secp256k1};
use zksync_utils::u256_to_h256;

pub trait ToU256 {
    fn to_u256(&self) -> U256;
}

impl ToU256 for Uint<256, 4> {
    fn to_u256(&self) -> U256 {
        U256(self.into_limbs())
    }
}

pub trait ToH256 {
    fn to_h256(&self) -> H256;
}

impl ToH256 for Uint<256, 4> {
    fn to_h256(&self) -> H256 {
        u256_to_h256(self.to_u256())
    }
}

pub trait ToH160 {
    fn to_h160(&self) -> H160;
}

impl ToH160 for Address {
    fn to_h160(&self) -> H160 {
        H160::from_slice(self.as_slice())
    }
}

//from crates/cheatcodes/src/utils.rs
pub(super) fn parse_private_key(private_key: &Uint<256, 4>) -> Option<[u8; 32]> {
    //private key cannot be 0
    if *private_key == Uint::ZERO {
        return None
    }

    // private key must be less than the secp256k1 curve order
    // (115792089237316195423570985008687907852837564279074904382605163141518161494337)
    if *private_key >= Uint::<256, 4>::from_limbs(*Secp256k1::ORDER.as_words()) {
        return None
    }

    Some(private_key.to_be_bytes())
}

pub(super) fn parse_wallet(private_key: &Uint<256, 4>) -> Option<LocalWallet> {
    parse_private_key(private_key).and_then(|b| LocalWallet::from_bytes(&b).ok())
}
