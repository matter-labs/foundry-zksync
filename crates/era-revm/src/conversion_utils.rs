/// Conversion between REVM units and zkSync units.
use revm::primitives::U256 as revmU256;
use revm::primitives::{Address, B256};

use zksync_basic_types::{H160, H256, U256};
use zksync_utils::h256_to_u256;

pub fn address_to_h160(i: Address) -> H160 {
    H160::from(i.0 .0)
}

pub fn h160_to_address(i: H160) -> Address {
    i.as_fixed_bytes().into()
}

pub fn u256_to_b256(i: U256) -> B256 {
    let mut payload: [u8; 32] = [0; 32];
    i.to_big_endian(&mut payload);
    B256::from_slice(&payload)
}

pub fn u256_to_revm_u256(i: U256) -> revmU256 {
    let mut payload: [u8; 32] = [0; 32];
    i.to_big_endian(&mut payload);
    revmU256::from_be_bytes(payload)
}

pub fn revm_u256_to_u256(i: revmU256) -> U256 {
    U256::from_big_endian(&i.to_be_bytes::<32>())
}

pub fn revm_u256_to_h256(i: revmU256) -> H256 {
    i.to_be_bytes::<32>().into()
}

pub fn h256_to_revm_u256(i: H256) -> revmU256 {
    u256_to_revm_u256(h256_to_u256(i))
}

pub fn h256_to_b256(i: H256) -> B256 {
    i.to_fixed_bytes().into()
}

pub fn h256_to_h160(i: &H256) -> H160 {
    H160::from_slice(&i.0[12..32])
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use zksync_utils::u256_to_h256;

    use super::*;

    #[test]
    fn test_160_conversion() {
        let b = Address::from_str("0x000000000000000000000000000000000000800b").unwrap();
        let h = address_to_h160(b);
        assert_eq!(h.to_string(), "0x0000…800b");
        let b2 = h160_to_address(h);
        assert_eq!(b, b2);
    }

    #[test]
    fn test_256_conversion() {
        let h =
            H256::from_str("0xb99acb716b354b9be88d3eaba99ad36792ccdd4349404cbb812adf0b0b14d601")
                .unwrap();
        let b = h256_to_b256(h);
        assert_eq!(
            b.to_string(),
            "0xb99acb716b354b9be88d3eaba99ad36792ccdd4349404cbb812adf0b0b14d601"
        );
        let u = h256_to_u256(h);
        assert_eq!(
            u.to_string(),
            "83951375548152864551218308881540843734370423742152710934930688330188941743617"
        );

        let revm_u = u256_to_revm_u256(u);
        assert_eq!(
            revm_u.to_string(),
            "83951375548152864551218308881540843734370423742152710934930688330188941743617"
        );
        assert_eq!(u, revm_u256_to_u256(revm_u));

        assert_eq!(h, revm_u256_to_h256(revm_u));

        assert_eq!(h, u256_to_h256(u));
    }
}
