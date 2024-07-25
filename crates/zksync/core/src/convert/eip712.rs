use alloy_consensus::{SignableTransaction, Transaction};
use alloy_dyn_abi::TypedData;
use alloy_primitives::{bytes::BufMut, Signature, B256};
/// Conversion between ethers and alloy for EIP712 items
use alloy_sol_types::Eip712Domain as AlloyEip712Domain;
use zksync_web3_rs::{
    eip712::Eip712Transaction,
    types::transaction::eip712::{
        encode_type, EIP712Domain as EthersEip712Domain, Eip712 as EthersEip712, Eip712DomainType,
        Types,
    },
};

use super::{ConvertAddress, ConvertH160, ConvertRU256, ConvertU256};

/// Convert between Eip712Domain types
pub trait ConvertEIP712Domain {
    /// Cast to ethers-rs's Eip712Domain
    fn to_ethers(self) -> EthersEip712Domain;

    /// Cast to alloy-rs's Eip712Domain
    fn to_alloy(self) -> AlloyEip712Domain;
}

impl ConvertEIP712Domain for AlloyEip712Domain {
    fn to_ethers(self) -> EthersEip712Domain {
        EthersEip712Domain {
            name: self.name.map(Into::into),
            version: self.version.map(Into::into),
            chain_id: self.chain_id.map(ConvertRU256::to_u256),
            verifying_contract: self.verifying_contract.map(ConvertAddress::to_h160),
            salt: self.salt.map(Into::into),
        }
    }

    fn to_alloy(self) -> AlloyEip712Domain {
        self
    }
}

impl ConvertEIP712Domain for EthersEip712Domain {
    fn to_ethers(self) -> EthersEip712Domain {
        self
    }

    fn to_alloy(self) -> AlloyEip712Domain {
        AlloyEip712Domain::new(
            self.name.map(Into::into),
            self.version.map(Into::into),
            self.chain_id.map(ConvertU256::to_ru256),
            self.verifying_contract.map(ConvertH160::to_address),
            self.salt.map(Into::into),
        )
    }
}

/// Wrapper around [`Eip712Transaction`] implementing [`SignableTransaction`]
pub struct Eip712SignableTransaction(Eip712Transaction);

impl Transaction for Eip712SignableTransaction {
    fn chain_id(&self) -> Option<alloy_primitives::ChainId> {
        Some(self.0.chain_id.as_u64())
    }

    fn nonce(&self) -> u64 {
        self.0.nonce.as_u64()
    }

    fn gas_limit(&self) -> u128 {
        self.0.gas_limit.as_u128()
    }

    fn gas_price(&self) -> Option<u128> {
        None
    }

    fn to(&self) -> alloy_primitives::TxKind {
        alloy_primitives::TxKind::Call(self.0.to.to_address())
    }

    fn value(&self) -> alloy_primitives::U256 {
        self.0.value.to_ru256()
    }

    fn input(&self) -> &[u8] {
        self.0.data.as_ref()
    }
}

impl SignableTransaction<Signature> for Eip712SignableTransaction {
    fn set_chain_id(&mut self, chain_id: alloy_primitives::ChainId) {
        self.0.chain_id = chain_id.into();
    }

    fn encode_for_signing(&self, out: &mut dyn BufMut) {
        out.put_u8(0x19);
        out.put_u8(0x01);

        let domain_separator = self.0.domain_separator().expect("able to get domain separator");
        out.put_slice(&domain_separator);

        let struct_hash = self.0.struct_hash().expect("able to get struct hash");
        out.put_slice(&struct_hash);
    }

    fn payload_len_for_signature(&self) -> usize {
        2 + 32 + 32
    }

    fn into_signed(self, signature: Signature) -> alloy_consensus::Signed<Self, Signature>
    where
        Self: Sized,
    {
        let hash = self.0.encode_eip712().map(B256::from).expect("able to encode EIP712 hash");
        alloy_consensus::Signed::new_unchecked(self, signature, hash)
    }
}

/// Convert to [`SignableTransaction`]
pub trait ToSignable<S> {
    /// Type to convert to
    type Signable: SignableTransaction<S>;

    /// Perform conversion
    fn to_signable_tx(self) -> Self::Signable;
}

impl ToSignable<Signature> for Eip712Transaction {
    type Signable = Eip712SignableTransaction;

    fn to_signable_tx(self) -> Self::Signable {
        Eip712SignableTransaction(self)
    }
}

/// Convert to [`TypedData`]
pub trait ToTypedData {
    /// Convert item to [`TypedData`]
    fn to_typed_data(self) -> TypedData;
}

impl ToTypedData for Eip712Transaction {
    fn to_typed_data(self) -> TypedData {
        use alloy_dyn_abi::*;

        let types = eip712_transaction_types();
        let primary_type = types.first_key_value().unwrap().0.clone();

        let domain = EthersEip712::domain(&self).expect("Eip712Transaction has domain").to_alloy();

        let message = serde_json::to_value(&self).expect("able to serialize as json");

        let encode_type = encode_type(&primary_type, &types).expect("able to encodeType");

        let resolver = {
            let mut resolver = Resolver::default();
            resolver.ingest_string(&encode_type).expect("able to ingest encodeType");
            resolver
        };

        TypedData { domain, resolver, primary_type, message }
    }
}

//zksync_web3_rs::eip712::transaction
fn eip712_transaction_types() -> Types {
    let mut types = Types::new();

    types.insert(
        "Transaction".to_owned(),
        vec![
            Eip712DomainType { name: "txType".to_owned(), r#type: "uint256".to_owned() },
            Eip712DomainType { name: "from".to_owned(), r#type: "uint256".to_owned() },
            Eip712DomainType { name: "to".to_owned(), r#type: "uint256".to_owned() },
            Eip712DomainType { name: "gasLimit".to_owned(), r#type: "uint256".to_owned() },
            Eip712DomainType {
                name: "gasPerPubdataByteLimit".to_owned(),
                r#type: "uint256".to_owned(),
            },
            Eip712DomainType { name: "maxFeePerGas".to_owned(), r#type: "uint256".to_owned() },
            Eip712DomainType {
                name: "maxPriorityFeePerGas".to_owned(),
                r#type: "uint256".to_owned(),
            },
            Eip712DomainType { name: "paymaster".to_owned(), r#type: "uint256".to_owned() },
            Eip712DomainType { name: "nonce".to_owned(), r#type: "uint256".to_owned() },
            Eip712DomainType { name: "value".to_owned(), r#type: "uint256".to_owned() },
            Eip712DomainType { name: "data".to_owned(), r#type: "bytes".to_owned() },
            Eip712DomainType { name: "factoryDeps".to_owned(), r#type: "bytes32[]".to_owned() },
            Eip712DomainType { name: "paymasterInput".to_owned(), r#type: "bytes".to_owned() },
        ],
    );
    types
}
