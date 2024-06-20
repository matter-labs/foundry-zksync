//! ZKSolc module.
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    str::FromStr,
};

use foundry_compilers::{
    zksync::compile::output::ProjectCompileOutput as ZkProjectCompileOutput, Artifact,
    ArtifactOutput, ConfigurableArtifacts, ProjectCompileOutput, ProjectPathsConfig,
};

use alloy_primitives::{keccak256, B256};
use tracing::debug;
use zksync_types::H256;

/// Represents a zkSync compiled contract
#[derive(Debug, Default, Clone)]
pub struct ZkContract {
    /// Deployed bytecode hash with zksolc
    pub bytecode_hash: H256,
    /// Deployed bytecode with zksolc
    pub deployed_bytecode: Vec<u8>,
    /// Deployed bytecode factory deps
    pub factory_deps: Vec<Vec<u8>>,
}

/// Represents an EVM compiled contract
#[derive(Debug, Default, Clone)]
pub struct EvmContract {
    /// Deployed bytecode hash with solc
    pub bytecode_hash: B256,
    /// Deployed bytecode with solc
    pub deployed_bytecode: Vec<u8>,
    /// Bytecode with solc
    pub bytecode: Vec<u8>,
}

/// Defines a contract that has been dual compiled with both zksolc and solc
#[derive(Debug, Default, Clone)]
pub struct DualCompiledContract {
    /// Contract name
    pub name: String,
    /// Contract source path (if available)
    pub path: Option<PathBuf>,

    /// Will be `Some` if the contract was compiled for zksync
    pub zk: Option<ZkContract>,
    /// Will be `Some` if the contract was compiled for EVM
    pub evm: Option<EvmContract>,
}

impl DualCompiledContract {
    /// Instantiate a new full DualCompiledContract
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        path: Option<PathBuf>,
        zk_bytecode_hash: H256,
        zk_deployed_bytecode: Vec<u8>,
        zk_factory_deps: Vec<Vec<u8>>,
        evm_bytecode_hash: B256,
        evm_deployed_bytecode: Vec<u8>,
        evm_bytecode: Vec<u8>,
    ) -> Self {
        Self {
            name,
            path,
            zk: Some(ZkContract {
                bytecode_hash: zk_bytecode_hash,
                deployed_bytecode: zk_deployed_bytecode,
                factory_deps: zk_factory_deps,
            }),
            evm: Some(EvmContract {
                bytecode_hash: evm_bytecode_hash,
                deployed_bytecode: evm_deployed_bytecode,
                bytecode: evm_bytecode,
            }),
        }
    }
}

/// A collection of `[DualCompiledContract]`s
#[derive(Debug, Default, Clone)]
pub struct DualCompiledContracts {
    contracts: Vec<DualCompiledContract>,
}

impl DualCompiledContracts {
    /// Creates a collection of `[DualCompiledContract]`s from the provided solc output.
    pub fn new_solc(output: &ProjectCompileOutput, layout: &ProjectPathsConfig) -> Self {
        let mut this = Self::default();

        let output_artifacts = output
            .cached_artifacts()
            .artifact_files()
            .chain(output.compiled_artifacts().artifact_files())
            .filter_map(|artifact| {
                ConfigurableArtifacts::contract_name(&artifact.file)
                    .map(|name| (name, (&artifact.file, &artifact.artifact)))
            });

        for (contract_name, (artifact_path, artifact)) in output_artifacts {
            let contract_file = artifact_path
                .strip_prefix(&layout.artifacts)
                .unwrap_or_else(|_| {
                    panic!(
                        "failed stripping artifact path '{:?}' from '{:?}'",
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
                    this.push(DualCompiledContract {
                        name: contract_name,
                        path: Some(contract_file),
                        zk: None,
                        evm: Some(EvmContract {
                            bytecode: bytecode.to_vec(),
                            deployed_bytecode: deployed_bytecode.to_vec(),
                            bytecode_hash: keccak256(deployed_bytecode),
                        }),
                    });
                }
            }
        }

        this
    }

    /// Creates a collection of `[DualCompiledContract]`s from the provided zksolc output.
    pub fn new_zksolc(zk_output: &ZkProjectCompileOutput, layout: &ProjectPathsConfig) -> Self {
        let mut this = Self::default();

        let zk_output_artifacts = zk_output
            .cached_artifacts()
            .artifact_files()
            .chain(zk_output.compiled_artifacts().artifact_files())
            .filter_map(|artifact| {
                ConfigurableArtifacts::contract_name(&artifact.file)
                    .map(|name| (name, (&artifact.file, &artifact.artifact)))
            });

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
                let bytes = bytecode.object.clone().into_bytes().unwrap();
                zksolc_all_bytecodes.insert(hash.clone(), bytes.to_vec());
            }
        }

        for (contract_name, (artifact_path, artifact)) in zk_output_artifacts {
            let contract_file = artifact_path
                .strip_prefix(&layout.zksync_artifacts)
                .unwrap_or_else(|_| {
                    panic!(
                        "failed stripping artifact path '{:?}' from '{:?}'",
                        layout.zksync_artifacts, artifact_path
                    )
                })
                .to_path_buf();

            let maybe_bytecode = &artifact.bytecode;
            let maybe_hash = &artifact.hash;
            let maybe_factory_deps = &artifact.factory_dependencies;
            if let (Some(bytecode), Some(hash), Some(factory_deps_map)) =
                (maybe_bytecode, maybe_hash, maybe_factory_deps)
            {
                // TODO: we can do this because no bytecode object could be unlinked
                // at this stage for zksolc, and BytecodeObject as ref will get the bytecode
                // bytes. However, we should check and
                // handle errors in case an Unlinked BytecodeObject gets
                // here somehow
                let bytecode_vec = bytecode.object.clone().into_bytes().unwrap().to_vec();
                let mut factory_deps_vec: Vec<Vec<u8>> = factory_deps_map
                    .keys()
                    .map(|factory_hash| zksolc_all_bytecodes.get(factory_hash).unwrap())
                    .cloned()
                    .collect();

                factory_deps_vec.push(bytecode_vec.clone());

                let zk_contract = Some(ZkContract {
                    bytecode_hash: H256::from_str(hash).unwrap(),
                    deployed_bytecode: bytecode_vec,
                    factory_deps: factory_deps_vec,
                });

                this.push(DualCompiledContract {
                    name: contract_name,
                    path: Some(contract_file),
                    zk: zk_contract,
                    evm: None,
                });
            }
        }

        this
    }

    /// Merge 2 [`DualCompiledContracts`] instances
    ///
    /// When a contract in the collections have the same components, the `other`'s is used
    fn _merge(this: DualCompiledContracts, mut other: DualCompiledContracts) -> Self {
        this.into_iter().for_each(|contract| {
            let Some(path) = contract.path.as_ref() else {
                other.push(contract);
                return;
            };

            if let Some(existing) = other.edit_by_path(path) {
                if let Some(evm) = contract.evm {
                    existing.evm.get_or_insert(evm);
                }

                if let Some(zk) = contract.zk {
                    existing.zk.get_or_insert(zk);
                }
            } else {
                other.push(contract);
            }
        });

        other
    }

    /// Creates a collection of `[DualCompiledContract]`s from the provided solc and zksolc output.
    pub fn new_dual(
        output: &ProjectCompileOutput,
        zk_output: &ZkProjectCompileOutput,
        layout: &ProjectPathsConfig,
    ) -> Self {
        let this = Self::new_solc(output, layout);
        let other = Self::new_zksolc(zk_output, layout);

        Self::_merge(this, other)
    }

    /// Finds a contract matching the contract path
    pub fn find_by_path(&self, path: impl AsRef<Path>) -> Option<&DualCompiledContract> {
        self.contracts.iter().find(|contract| contract.path.as_deref() == Some(path.as_ref()))
    }

    /// Finds a contract matching the ZK deployed bytecode
    pub fn find_by_zk_deployed_bytecode(&self, bytecode: &[u8]) -> Option<&DualCompiledContract> {
        self.contracts.iter().find(|contract| {
            if let Some(zk) = &contract.zk {
                bytecode.starts_with(&zk.deployed_bytecode)
            } else {
                false
            }
        })
    }

    /// Finds a contract matching the EVM bytecode
    pub fn find_by_evm_bytecode(&self, bytecode: &[u8]) -> Option<&DualCompiledContract> {
        self.contracts.iter().find(|contract| {
            if let Some(evm) = &contract.evm {
                bytecode.starts_with(&evm.bytecode)
            } else {
                false
            }
        })
    }

    /// Finds a contract matching the ZK bytecode hash
    pub fn find_by_zk_bytecode_hash(&self, code_hash: H256) -> Option<&DualCompiledContract> {
        self.contracts.iter().find(|contract| {
            if let Some(zk) = &contract.zk {
                code_hash == zk.bytecode_hash
            } else {
                false
            }
        })
    }

    /// Finds a contract own and nested factory deps
    pub fn fetch_all_factory_deps(&self, root: &DualCompiledContract) -> Vec<Vec<u8>> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        for dep in root.zk.as_ref().map(|zk| &zk.factory_deps).into_iter().flatten() {
            queue.push_back(dep);
        }

        while let Some(dep) = queue.pop_front() {
            // try to insert in the list of visited, if it's already present, skip
            if visited.insert(dep) {
                if let Some(contract) = self.find_by_zk_deployed_bytecode(dep) {
                    let factory_deps = &contract.zk.as_ref().unwrap().factory_deps;

                    debug!(
                        name = contract.name,
                        deps = factory_deps.len(),
                        "new factory depdendency"
                    );

                    for nested_dep in factory_deps.iter() {
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

    /// Edit a [`DualCompiledContract`] entry by looking up the contract path
    fn edit_by_path(
        &mut self,
        contract_file: impl AsRef<Path>,
    ) -> Option<&mut DualCompiledContract> {
        self.contracts
            .iter_mut()
            .find(|contract| contract.path.as_deref() == Some(contract_file.as_ref()))
    }
}

impl IntoIterator for DualCompiledContracts {
    type IntoIter = <Vec<DualCompiledContract> as IntoIterator>::IntoIter;
    type Item = <Self::IntoIter as Iterator>::Item;

    fn into_iter(self) -> Self::IntoIter {
        self.contracts.into_iter()
    }
}
