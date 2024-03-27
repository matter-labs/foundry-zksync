//! ZKSolc module.

mod compile;
mod config;
mod factory_deps;
mod manager;

use std::collections::HashMap;

pub use compile::*;
pub use config::*;
pub use factory_deps::*;
use foundry_compilers::{Artifact, ProjectCompileOutput};
pub use manager::*;

use alloy_primitives::{keccak256, B256};
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

impl DualCompiledContract {
    /// Creates a list of [DualCompiledContract]s from the provided solc and zksolc output.
    pub fn compile_all(output: &ProjectCompileOutput, zk_output: &ProjectCompileOutput) -> Vec<Self> {
        let mut dual_compiled_contracts = vec![];
        let mut solc_bytecodes = HashMap::new();
        for (contract_name, artifact) in output.artifacts() {
            let contract_name =
                contract_name.split('.').next().expect("name cannot be empty").to_string();
            let deployed_bytecode = artifact.get_deployed_bytecode();
            let deployed_bytecode = deployed_bytecode
                .as_ref()
                .and_then(|d| d.bytecode.as_ref().and_then(|b| b.object.as_bytes()));
            let bytecode = artifact.get_bytecode().and_then(|b| b.object.as_bytes().cloned());
            if let Some(bytecode) = bytecode {
                if let Some(deployed_bytecode) = deployed_bytecode {
                    solc_bytecodes
                        .insert(contract_name.clone(), (bytecode, deployed_bytecode.clone()));
                }
            }
        }
        for (contract_name, artifact) in zk_output.artifacts() {
            let deployed_bytecode = artifact.get_deployed_bytecode();
            let deployed_bytecode = deployed_bytecode
                .as_ref()
                .and_then(|d| d.bytecode.as_ref().and_then(|b| b.object.as_bytes()));
            if let Some(deployed_bytecode) = deployed_bytecode {
                let packed_bytecode = PackedEraBytecode::from_vec(deployed_bytecode);
                if let Some((solc_bytecode, solc_deployed_bytecode)) =
                    solc_bytecodes.get(&contract_name)
                {
                    dual_compiled_contracts.push(DualCompiledContract {
                        name: contract_name,
                        zk_bytecode_hash: packed_bytecode.bytecode_hash(),
                        zk_deployed_bytecode: packed_bytecode.bytecode(),
                        evm_bytecode_hash: keccak256(solc_deployed_bytecode),
                        evm_bytecode: solc_bytecode.to_vec(),
                        evm_deployed_bytecode: solc_deployed_bytecode.to_vec(),
                    });
                }
            }
        }

        dual_compiled_contracts
    }
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
