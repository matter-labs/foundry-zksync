
use alloy_dyn_abi::TypedData;
/// Conversion between ethers and alloy for EIP712 items
use alloy_sol_types::{Eip712Domain as AlloyEip712Domain};
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
