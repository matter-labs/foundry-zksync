//! ZKSolc module.

mod compile;
mod config;
mod factory_deps;
mod manager;

use std::collections::{HashMap, HashSet, VecDeque};

pub use compile::*;
pub use config::*;
pub use factory_deps::*;
use foundry_compilers::{Artifact, ProjectCompileOutput};
pub use manager::*;

use alloy_primitives::{keccak256, B256};
use tracing::debug;
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
    /// Deployed bytecode factory deps
    pub zk_factory_deps: Vec<Vec<u8>>,
    /// Deployed bytecode hash with solc
    pub evm_bytecode_hash: B256,
    /// Deployed bytecode with solc
    pub evm_deployed_bytecode: Vec<u8>,
    /// Bytecode with solc
    pub evm_bytecode: Vec<u8>,
}

/// A collection of `[DualCompiledContract]`s
#[derive(Debug, Default, Clone)]
pub struct DualCompiledContracts {
    contracts: Vec<DualCompiledContract>,
}

impl DualCompiledContracts {
    /// Creates a collection of `[DualCompiledContract]`s from the provided solc and zksolc output.
    pub fn new(output: &ProjectCompileOutput, zk_output: &ProjectCompileOutput) -> Self {
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
                        zk_factory_deps: packed_bytecode.factory_deps(),
                        evm_bytecode_hash: keccak256(solc_deployed_bytecode),
                        evm_bytecode: solc_bytecode.to_vec(),
                        evm_deployed_bytecode: solc_deployed_bytecode.to_vec(),
                    });
                }
            }
        }

        Self { contracts: dual_compiled_contracts }
    }

    /// Finds a contract matching the ZK deployed bytecode
    pub fn find_by_zk_deployed_bytecode(&self, bytecode: &[u8]) -> Option<&DualCompiledContract> {
        self.contracts.iter().find(|contract| bytecode.starts_with(&contract.zk_deployed_bytecode))
    }

    /// Finds a contract matching the EVM bytecode
    pub fn find_by_evm_bytecode(&self, bytecode: &[u8]) -> Option<&DualCompiledContract> {
        self.contracts.iter().find(|contract| bytecode.starts_with(&contract.evm_bytecode))
    }

    /// Finds a contract matching the ZK bytecode hash
    pub fn find_by_zk_bytecode_hash(&self, code_hash: H256) -> Option<&DualCompiledContract> {
        self.contracts.iter().find(|contract| code_hash == contract.zk_bytecode_hash)
    }

    /// Finds a contract own and nested factory deps
    pub fn fetch_all_factory_deps(&self, root: &DualCompiledContract) -> Vec<Vec<u8>> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        for dep in &root.zk_factory_deps {
            queue.push_back(dep);
        }

        while let Some(dep) = queue.pop_front() {
            // try to insert in the list of visited, if it's already present, skip
            if visited.insert(dep) {
                if let Some(contract) = self.find_by_zk_deployed_bytecode(dep) {
                    debug!(
                        name = contract.name,
                        deps = contract.zk_factory_deps.len(),
                        "new factory dependency"
                    );

                    for nested_dep in &contract.zk_factory_deps {
                        // check that the nested dependency is inserted
                        if !visited.contains(nested_dep) {
                            // if not, add it to queue for processing
                            queue.push_back(nested_dep);
                        }
                    }
                }
            }
        }

        visited.into_iter().cloned().collect()
    }

    /// Returns an iterator over all `[DualCompiledContract]`s in the collection
    pub fn iter(&self) -> impl Iterator<Item = &DualCompiledContract> {
        self.contracts.iter()
    }

    /// Adds a new `[DualCompiledContract]` to the collection
    pub fn push(&mut self, contract: DualCompiledContract) {
        self.contracts.push(contract);
    }
}
