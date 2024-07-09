/// Conversion between REVM units and zkSync units.
use revm::primitives::U256 as rU256;
use revm::primitives::{Address, B256};

use zksync_basic_types::{H160, H256, U256};
use zksync_utils::{address_to_h256, h256_to_u256, u256_to_h256};

use alloy_primitives::{Bytes as AlloyBytes, Signature as AlloySignature};
use zksync_web3_rs::types::{Bytes as ZkBytes, Signature as ZkSignature};

mod eip712;
pub use eip712::*;

/// Conversions from [U256]
pub trait ConvertU256 {
    /// Convert to [rU256]
    fn to_ru256(self) -> rU256;

    /// Convert to [B256]
    fn to_b256(self) -> B256;

    /// Convert to [H256]
    fn to_h256(self) -> H256;
}

impl ConvertU256 for U256 {
    fn to_ru256(self) -> rU256 {
        let mut payload: [u8; 32] = [0; 32];
        self.to_big_endian(&mut payload);
        rU256::from_be_bytes(payload)
    }

    fn to_b256(self) -> B256 {
        let mut payload: [u8; 32] = [0; 32];
        self.to_big_endian(&mut payload);
        B256::from_slice(&payload)
    }

    /// Convert to [H256]
    fn to_h256(self) -> H256 {
        u256_to_h256(self)
    }
}

/// Conversions from [rU256]
pub trait ConvertRU256 {
    /// Convert to [U256]
    fn to_u256(self) -> U256;

    /// Convert to [H256]
    fn to_h256(self) -> H256;
}

impl ConvertRU256 for rU256 {
    fn to_u256(self) -> U256 {
        U256::from_big_endian(self.to_be_bytes::<32>().as_slice())
    }

    fn to_h256(self) -> H256 {
        self.to_be_bytes::<32>().into()
    }
}

/// Conversions from [H256]
pub trait ConvertH256 {
    /// Convert to [rU256]
    fn to_ru256(self) -> rU256;

    /// Convert to [B256]
    fn to_b256(self) -> B256;

    /// Convert to [H160]
    fn to_h160(self) -> H160;
}

impl ConvertH256 for H256 {
    fn to_ru256(self) -> rU256 {
        h256_to_u256(self).to_ru256()
    }

    fn to_b256(self) -> B256 {
        self.to_fixed_bytes().into()
    }

    fn to_h160(self) -> H160 {
        H160::from_slice(&self.0[12..32])
    }
}

/// Conversions from [H160]
pub trait ConvertH160 {
    /// Convert to [Address]
    fn to_address(self) -> Address;

    /// Convert to [H256]
    fn to_h256(self) -> H256;
}

impl ConvertH160 for H160 {
    fn to_address(self) -> Address {
        self.as_fixed_bytes().into()
    }

    fn to_h256(self) -> H256 {
        address_to_h256(&self)
    }
}

/// Conversions from [Address]
pub trait ConvertAddress {
    /// Convert to [rU256]
    fn to_ru256(self) -> rU256;

    /// Convert to [H256]
    fn to_h256(self) -> H256;

    /// Convert to [H160]
    fn to_h160(self) -> H160;
}

impl ConvertAddress for Address {
    fn to_ru256(self) -> rU256 {
        let mut buffer = [0u8; 32];
        buffer[12..].copy_from_slice(self.as_slice());
        rU256::from_be_bytes(buffer)
    }

    fn to_h256(self) -> H256 {
        let mut buffer = [0u8; 32];
        buffer[12..].copy_from_slice(self.as_slice());
        H256(buffer)
    }

    fn to_h160(self) -> H160 {
        H160::from(self.0 .0)
    }
}

/// Conversions to/from [`ZkSignature`] & [`AlloySignature`]
pub trait ConvertSignature {
    /// Cast to [`ZkSignature`]
    fn to_ethers(self) -> ZkSignature;
    /// Cast to [`AlloySignature`]
    fn to_alloy(self) -> AlloySignature;
}

impl ConvertSignature for ZkSignature {
    fn to_ethers(self) -> ZkSignature {
        self
    }

    fn to_alloy(self) -> AlloySignature {
        AlloySignature::from_rs_and_parity(self.r.to_ru256(), self.s.to_ru256(), self.v).unwrap()
    }
}

impl ConvertSignature for AlloySignature {
    fn to_ethers(self) -> ZkSignature {
        let v = self.v();
        ZkSignature {
            r: self.r().to_u256(),
            s: self.s().to_u256(),
            v: v.y_parity_byte_non_eip155().unwrap_or(v.y_parity_byte()) as u64,
        }
    }

    fn to_alloy(self) -> AlloySignature {
        self
    }
}

/// Convert to/from [`AlloyBytes`] & [`ZkBytes`]
pub trait ConvertBytes {
    /// Convert to [`AlloyBytes`]
    fn to_alloy(self) -> AlloyBytes;
    /// Convert to [`ZkBytes`]
    fn to_ethers(self) -> ZkBytes;
}

impl ConvertBytes for AlloyBytes {
    fn to_alloy(self) -> AlloyBytes {
        self
    }

    fn to_ethers(self) -> ZkBytes {
        ZkBytes(self.0)
    }
}

impl ConvertBytes for ZkBytes {
    fn to_alloy(self) -> AlloyBytes {
        AlloyBytes(self.0)
    }

    fn to_ethers(self) -> ZkBytes {
        self
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_160_conversion() {
        let b = Address::from_str("0x000000000000000000000000000000000000800b").unwrap();
        let h = b.to_h160();
        assert_eq!(h.to_string(), "0x0000…800b");
        let b2 = h.to_address();
        assert_eq!(b, b2);
    }

    #[test]
    fn test_256_conversion() {
        let h =
            H256::from_str("0xb99acb716b354b9be88d3eaba99ad36792ccdd4349404cbb812adf0b0b14d601")
                .unwrap();
        let b = h.to_b256();
        assert_eq!(
            b.to_string(),
            "0xb99acb716b354b9be88d3eaba99ad36792ccdd4349404cbb812adf0b0b14d601"
        );
        let u = h256_to_u256(h);
        assert_eq!(
            u.to_string(),
            "83951375548152864551218308881540843734370423742152710934930688330188941743617"
        );

        let revm_u = u.to_ru256();
        assert_eq!(
            revm_u.to_string(),
            "83951375548152864551218308881540843734370423742152710934930688330188941743617"
        );
        assert_eq!(u, revm_u.to_u256());

        assert_eq!(h, revm_u.to_h256());

        assert_eq!(h, u.to_h256());
    }
}
