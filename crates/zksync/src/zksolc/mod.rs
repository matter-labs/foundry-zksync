//! ZKSolc module.

mod compile;
mod factory_deps;
mod manager;

use alloy_primitives::B256;
pub use compile::*;
pub use factory_deps::*;
pub use manager::*;
use zksync_types::H256;

/// Defines a contract that has been dual compiled with both zksolc and solc
#[derive(Debug, Default, Clone)]
pub struct DualCompiledContract {
    /// Contract name
    pub name: String,
    /// Deployed bytecode with zksolc
    pub zk_bytecode_hash: H256,
    /// Deployed bytecode hash with zksolc
    pub zk_deployed_bytecode: Vec<u8>,
    /// Deployed bytecode hash with solc
    pub evm_bytecode_hash: B256,
    /// Deployed bytecode with solc
    pub evm_deployed_bytecode: Vec<u8>,
    /// Bytecode with solc
    pub evm_bytecode: Vec<u8>,
}

/// Implements methods to look for contracts
pub trait FindContract {
    /// Finds a contract matching the EVM bytecode
    fn find_evm_bytecode(&self, bytecode: &[u8]) -> Option<&DualCompiledContract>;

    /// Finds a contract matching the ZK bytecode hash
    fn find_zk_bytecode_hash(&self, code_hash: H256) -> Option<&DualCompiledContract>;
}

impl FindContract for Vec<DualCompiledContract> {
    fn find_evm_bytecode(&self, bytecode: &[u8]) -> Option<&DualCompiledContract> {
        self.iter().find(|contract| bytecode.starts_with(&contract.evm_bytecode))
    }

    fn find_zk_bytecode_hash(&self, code_hash: H256) -> Option<&DualCompiledContract> {
        self.iter().find(|contract| code_hash == contract.zk_bytecode_hash)
    }
}
