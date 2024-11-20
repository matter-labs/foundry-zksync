//! ZKSolc module.
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    str::FromStr,
};

use foundry_compilers::{
    solc::SolcLanguage, zksync::compile::output::ProjectCompileOutput as ZkProjectCompileOutput,
    Artifact, ArtifactId, ArtifactOutput, ConfigurableArtifacts, ProjectCompileOutput,
    ProjectPathsConfig,
};

use alloy_primitives::{keccak256, B256};
use tracing::debug;
use zksync_types::H256;

/// Represents the type of contract (ZK or EVM)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractType {
    /// ZkSolc compiled contract
    ZK,
    /// Solc compiled contract
    EVM,
}

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

/// Couple contract type with contract and init code
pub struct FindBytecodeResult<'a> {
    r#type: ContractType,
    contract: &'a DualCompiledContract,
    init_code: &'a [u8],
}

impl<'a> FindBytecodeResult<'a> {
    /// Retrieve the found contract
    pub fn contract(self) -> &'a DualCompiledContract {
        self.contract
    }

    /// Retrieve the correct constructor args
    pub fn constructor_args(&self) -> &'a [u8] {
        match self.r#type {
            ContractType::ZK => &self.init_code[self.contract.zk_deployed_bytecode.len()..],
            ContractType::EVM => &self.init_code[self.contract.evm_bytecode.len()..],
        }
    }
}

/// A collection of `[DualCompiledContract]`s
#[derive(Debug, Default, Clone)]
pub struct DualCompiledContracts {
    contracts: Vec<DualCompiledContract>,
    /// ZKvm artifacts path
    pub zk_artifact_path: PathBuf,
    /// EVM artifacts path
    pub evm_artifact_path: PathBuf,
}

impl DualCompiledContracts {
    /// Creates a collection of `[DualCompiledContract]`s from the provided solc and zksolc output.
    pub fn new(
        output: &ProjectCompileOutput,
        zk_output: &ZkProjectCompileOutput,
        layout: &ProjectPathsConfig,
        zk_layout: &ProjectPathsConfig<SolcLanguage>,
    ) -> Self {
        let mut dual_compiled_contracts = vec![];
        let mut solc_bytecodes = HashMap::new();

        let output_artifacts = output
            .cached_artifacts()
            .artifact_files()
            .chain(output.compiled_artifacts().artifact_files())
            .filter_map(|artifact| {
                ConfigurableArtifacts::contract_name(&artifact.file)
                    .map(|name| (name, (&artifact.file, &artifact.artifact)))
            });
        let zk_output_artifacts = zk_output
            .cached_artifacts()
            .artifact_files()
            .chain(zk_output.compiled_artifacts().artifact_files())
            .filter_map(|artifact| {
                ConfigurableArtifacts::contract_name(&artifact.file)
                    .map(|name| (name, (&artifact.file, &artifact.artifact)))
            });

        for (_contract_name, (artifact_path, artifact)) in output_artifacts {
            let contract_file = artifact_path
                .strip_prefix(&layout.artifacts)
                .unwrap_or_else(|_| {
                    panic!(
                        "failed stripping solc artifact path '{:?}' from '{:?}'",
                        layout.artifacts, artifact_path
                    )
                })
                .to_path_buf();

            let deployed_bytecode = artifact.get_deployed_bytecode();
            let deployed_bytecode = deployed_bytecode
                .as_ref()
                .and_then(|d| d.bytecode.as_ref().and_then(|b| b.object.as_bytes()));
            let bytecode = artifact.get_bytecode().and_then(|b| b.object.as_bytes().cloned());
            if let Some(bytecode) = bytecode {
                if let Some(deployed_bytecode) = deployed_bytecode {
                    solc_bytecodes.insert(contract_file, (bytecode, deployed_bytecode.clone()));
                }
            }
        }

        // DualCompiledContracts uses a vec of bytecodes as factory deps field vs
        // the <hash, name> map zksolc outputs, hence we need all bytecodes upfront to
        // then do the conversion
        let mut zksolc_all_bytecodes: HashMap<String, Vec<u8>> = Default::default();
        for (_, zk_artifact) in zk_output.artifacts() {
            if let (Some(hash), Some(bytecode)) = (&zk_artifact.hash, &zk_artifact.bytecode) {
                // TODO: we can do this because no bytecode object could be unlinked
                // at this stage for zksolc, and BytecodeObject as ref will get the bytecode bytes.
                // We should be careful however and check/handle errors in
                // case an Unlinked BytecodeObject gets here somehow
                let bytes = bytecode.object().into_bytes().unwrap();
                zksolc_all_bytecodes.insert(hash.clone(), bytes.to_vec());
            }
        }

        for (contract_name, (artifact_path, artifact)) in zk_output_artifacts {
            let contract_file = artifact_path
                .strip_prefix(&zk_layout.artifacts)
                .unwrap_or_else(|_| {
                    panic!(
                        "failed stripping zksolc artifact path '{:?}' from '{:?}'",
                        zk_layout.artifacts, artifact_path
                    )
                })
                .to_path_buf();

            let maybe_bytecode = &artifact.bytecode;
            let maybe_hash = &artifact.hash;
            let maybe_factory_deps = &artifact.factory_dependencies;

            if let (Some(bytecode), Some(hash), Some(factory_deps_map)) =
                (maybe_bytecode, maybe_hash, maybe_factory_deps)
            {
                if let Some((solc_bytecode, solc_deployed_bytecode)) =
                    solc_bytecodes.get(&contract_file)
                {
                    // TODO: we can do this because no bytecode object could be unlinked
                    // at this stage for zksolc, and BytecodeObject as ref will get the bytecode
                    // bytes. However, we should check and
                    // handle errors in case an Unlinked BytecodeObject gets
                    // here somehow
                    let bytecode_vec = bytecode.object().into_bytes().unwrap().to_vec();
                    let mut factory_deps_vec: Vec<Vec<u8>> = factory_deps_map
                        .keys()
                        .map(|factory_hash| {
                            zksolc_all_bytecodes.get(factory_hash).unwrap_or_else(|| {
                                panic!("failed to find zksolc artifact with hash {factory_hash:?}")
                            })
                        })
                        .cloned()
                        .collect();

                    factory_deps_vec.push(bytecode_vec.clone());

                    dual_compiled_contracts.push(DualCompiledContract {
                        name: contract_name,
                        zk_bytecode_hash: H256::from_str(hash).unwrap(),
                        zk_deployed_bytecode: bytecode_vec,
                        zk_factory_deps: factory_deps_vec,
                        evm_bytecode_hash: keccak256(solc_deployed_bytecode),
                        evm_bytecode: solc_bytecode.to_vec(),
                        evm_deployed_bytecode: solc_deployed_bytecode.to_vec(),
                    });
                } else {
                    tracing::error!("matching solc artifact not found for {contract_file:?}");
                }
            }
        }

        Self {
            contracts: dual_compiled_contracts,
            zk_artifact_path: zk_layout.artifacts.clone(),
            evm_artifact_path: layout.artifacts.clone(),
        }
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

    /// Find a contract matching the given bytecode, whether it's EVM or ZK.
    ///
    /// Will prioritize longest match
    pub fn find_bytecode<'a: 'b, 'b>(
        &'a self,
        init_code: &'b [u8],
    ) -> Option<FindBytecodeResult<'b>> {
        let evm = self.find_by_evm_bytecode(init_code).map(|evm| (ContractType::EVM, evm));
        let zk = self.find_by_zk_deployed_bytecode(init_code).map(|evm| (ContractType::ZK, evm));

        match (&evm, &zk) {
            (Some((_, evm)), Some((_, zk))) => {
                if zk.zk_deployed_bytecode.len() >= evm.evm_bytecode.len() {
                    Some(FindBytecodeResult { r#type: ContractType::ZK, contract: zk, init_code })
                } else {
                    Some(FindBytecodeResult { r#type: ContractType::EVM, contract: zk, init_code })
                }
            }
            _ => evm.or(zk).map(|(r#type, contract)| FindBytecodeResult {
                r#type,
                contract,
                init_code,
            }),
        }
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

    /// Returns the contract type (ZK or EVM) based on the artifact path
    pub fn get_contract_type_by_artifact(&self, artifact_id: &ArtifactId) -> Option<ContractType> {
        if artifact_id.path.starts_with(&self.zk_artifact_path) {
            Some(ContractType::ZK)
        } else if artifact_id.path.starts_with(&self.evm_artifact_path) {
            Some(ContractType::EVM)
        } else {
            panic!(
                "Unexpected artifact path: {:?}. Not found in ZK path {:?} or EVM path {:?}",
                artifact_id.path, self.zk_artifact_path, self.evm_artifact_path
            );
        }
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
