use revm::primitives::{ruint::Uint, Address};
use zksync_basic_types::{H160, H256, U256};
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
