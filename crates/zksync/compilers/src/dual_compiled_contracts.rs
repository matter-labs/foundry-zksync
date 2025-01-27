//! ZKSolc module.
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    str::FromStr,
};

use foundry_compilers::{
    info::ContractInfo, solc::SolcLanguage, Artifact, ArtifactId, ProjectCompileOutput,
    ProjectPathsConfig,
};

use alloy_primitives::{keccak256, B256};
use tracing::debug;
use zksync_types::H256;

use crate::compilers::{artifact_output::zk::ZkArtifactOutput, zksolc::ZkSolcCompiler};

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
    /// Deployed bytecode with zksolc
    pub zk_bytecode_hash: H256,
    /// Deployed bytecode hash with zksolc
    pub zk_deployed_bytecode: Vec<u8>,
    /// Bytecodes of the factory deps for zksolc's deployed bytecode
    pub zk_factory_deps: Vec<Vec<u8>>,
    /// Deployed bytecode hash with solc
    pub evm_bytecode_hash: B256,
    /// Deployed bytecode with solc
    pub evm_deployed_bytecode: Vec<u8>,
    /// Bytecode with solc
    pub evm_bytecode: Vec<u8>,
}

/// Indicates the type of match from a `find` search
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindMatchType {
    /// The result matched both path and name
    FullMatch,
    /// The result only matched the path
    Path,
    /// The result only matched the name
    Name,
}

/// Couple contract type with contract and init code
pub struct FindBytecodeResult<'a> {
    r#type: ContractType,
    info: &'a ContractInfo,
    contract: &'a DualCompiledContract,
    init_code: &'a [u8],
}

impl<'a> FindBytecodeResult<'a> {
    /// Retrieve the found contract's info
    pub fn info(&self) -> &'a ContractInfo {
        self.info
    }

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
    contracts: HashMap<ContractInfo, DualCompiledContract>,
    /// ZKvm artifacts path
    pub zk_artifact_path: PathBuf,
    /// EVM artifacts path
    pub evm_artifact_path: PathBuf,
}

impl DualCompiledContracts {
    /// Creates a collection of `[DualCompiledContract]`s from the provided solc and zksolc output.
    pub fn new(
        output: &ProjectCompileOutput,
        zk_output: &ProjectCompileOutput<ZkSolcCompiler, ZkArtifactOutput>,
        layout: &ProjectPathsConfig,
        zk_layout: &ProjectPathsConfig<SolcLanguage>,
    ) -> Self {
        let mut dual_compiled_contracts = HashMap::new();
        let mut solc_bytecodes = HashMap::new();

        let output_artifacts = output.artifact_ids().map(|(id, artifact)| {
            (
                ContractInfo {
                    name: id.name,
                    path: Some(id.source.to_string_lossy().into_owned()),
                },
                artifact,
            )
        });
        let zk_output_artifacts = zk_output.artifact_ids().map(|(id, artifact)| {
            (
                ContractInfo {
                    name: id.name,
                    path: Some(id.source.to_string_lossy().into_owned()),
                },
                artifact,
            )
        });

        for (contract_info, artifact) in output_artifacts {
            let deployed_bytecode = artifact.get_deployed_bytecode();
            let deployed_bytecode = deployed_bytecode
                .as_ref()
                .and_then(|d| d.bytecode.as_ref().and_then(|b| b.object.as_bytes()));
            let bytecode = artifact.get_bytecode().and_then(|b| b.object.as_bytes().cloned());
            if let Some(bytecode) = bytecode {
                if let Some(deployed_bytecode) = deployed_bytecode {
                    solc_bytecodes.insert(contract_info, (bytecode, deployed_bytecode.clone()));
                }
            }
        }

        // DualCompiledContracts uses a vec of bytecodes as factory deps field vs
        // the <hash, name> map zksolc outputs, hence we need all bytecodes upfront to
        // then do the conversion
        let mut zksolc_all_bytecodes: HashMap<String, Vec<u8>> = Default::default();
        for (_, zk_artifact) in zk_output.artifacts() {
            if let (Some(hash), Some(bytecode)) = (&zk_artifact.hash, &zk_artifact.bytecode) {
                // NOTE(zk): unlinked objects are _still_ encoded as valid hex
                // but the hash wouldn't be present
                let bytes = bytecode.object().into_bytes().unwrap();
                zksolc_all_bytecodes.insert(hash.clone(), bytes.to_vec());
            }
        }

        for (contract_info, artifact) in zk_output_artifacts {
            let maybe_bytecode = &artifact.bytecode;
            let maybe_hash = &artifact.hash;
            let maybe_factory_deps = &artifact.factory_dependencies;

            if let (Some(bytecode), Some(hash), Some(factory_deps_map)) =
                (maybe_bytecode, maybe_hash, maybe_factory_deps)
            {
                if let Some((solc_bytecode, solc_deployed_bytecode)) =
                    solc_bytecodes.get(&contract_info)
                {
                    // NOTE(zk): unlinked objects are _still_ encoded as valid hex
                    // but the hash wouldn't be present in the artifact
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

                    dual_compiled_contracts.insert(
                        contract_info,
                        DualCompiledContract {
                            zk_bytecode_hash: H256::from_str(hash).unwrap(),
                            zk_deployed_bytecode: bytecode_vec,
                            zk_factory_deps: factory_deps_vec,
                            evm_bytecode_hash: keccak256(solc_deployed_bytecode),
                            evm_bytecode: solc_bytecode.to_vec(),
                            evm_deployed_bytecode: solc_deployed_bytecode.to_vec(),
                        },
                    );
                } else {
                    tracing::error!("matching solc artifact not found for {contract_info:?}");
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
    pub fn find_by_zk_deployed_bytecode(
        &self,
        bytecode: &[u8],
    ) -> Option<(&ContractInfo, &DualCompiledContract)> {
        self.contracts
            .iter()
            .find(|(_, contract)| bytecode.starts_with(&contract.zk_deployed_bytecode))
    }

    /// Finds a contract matching the EVM bytecode
    pub fn find_by_evm_bytecode(
        &self,
        bytecode: &[u8],
    ) -> Option<(&ContractInfo, &DualCompiledContract)> {
        self.contracts.iter().find(|(_, contract)| bytecode.starts_with(&contract.evm_bytecode))
    }

    /// Finds a contract matching the ZK bytecode hash
    pub fn find_by_zk_bytecode_hash(
        &self,
        code_hash: H256,
    ) -> Option<(&ContractInfo, &DualCompiledContract)> {
        self.contracts.iter().find(|(_, contract)| code_hash == contract.zk_bytecode_hash)
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
            (Some((_, (evm_info, evm))), Some((_, (zk_info, zk)))) => {
                if zk.zk_deployed_bytecode.len() >= evm.evm_bytecode.len() {
                    Some(FindBytecodeResult {
                        r#type: ContractType::ZK,
                        contract: zk,
                        init_code,
                        info: zk_info,
                    })
                } else {
                    Some(FindBytecodeResult {
                        r#type: ContractType::EVM,
                        contract: zk,
                        init_code,
                        info: evm_info,
                    })
                }
            }
            _ => evm.or(zk).map(|(r#type, (info, contract))| FindBytecodeResult {
                r#type,
                info,
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
                if let Some((info, contract)) = self.find_by_zk_deployed_bytecode(dep) {
                    debug!(
                        name = info.name,
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
    pub fn iter(&self) -> impl Iterator<Item = (&ContractInfo, &DualCompiledContract)> {
        self.contracts.iter()
    }

    /// Adds a new `[DualCompiledContract]` to the collection
    ///
    /// Will replace any contract with matching `info`
    pub fn insert(&mut self, info: ContractInfo, contract: DualCompiledContract) {
        self.contracts.insert(info, contract);
    }

    /// Attempt reading an existing `[DualCompiledContract]`
    pub fn get(&self, info: &ContractInfo) -> Option<&DualCompiledContract> {
        self.contracts.get(info)
    }

    /// Search for matching contracts in the collection
    ///
    /// Contracts are ordered in descending best-fit order
    pub fn find<'a: 'b, 'b>(
        &'a self,
        path: Option<&'b str>,
        name: Option<&'b str>,
    ) -> impl Iterator<Item = (FindMatchType, &'a DualCompiledContract)> + 'b {
        let full_matches = self
            .contracts
            .iter()
            .filter(move |(info, _)| {
                // if user provides a path we should check that it matches
                // we check using `ends_with` to account for prefixes
                path.is_some_and(|needle|
                        info.path.as_ref()
                        .is_some_and(
                                |contract_path| contract_path.ends_with(needle)))
                // if user provides a name we should check that it matches
                && name.is_some_and(|name| name == info.name.as_str())
            })
            .map(|(_, contract)| (FindMatchType::FullMatch, contract));

        let path_matches = self
            .contracts
            .iter()
            .filter(move |(info, _)| {
                // if a path is provided, check that it matches
                // if no path is provided, don't match it
                path.is_some_and(|needle| {
                    info.path.as_ref().is_some_and(|contract_path| contract_path.ends_with(needle))
                })
            })
            .map(|(_, contract)| (FindMatchType::Path, contract));

        let name_matches = self
            .contracts
            .iter()
            .filter(move |(info, _)| {
                // if name is provided, check that it matches
                // if no name is provided, don't match it
                name.map(|name| name == info.name.as_str()).unwrap_or(false)
            })
            .map(|(_, contract)| (FindMatchType::Name, contract));

        full_matches.chain(path_matches).chain(name_matches)
    }

    /// Retrieves the length of the collection.
    pub fn len(&self) -> usize {
        self.contracts.len()
    }

    /// Retrieves if the collection is empty.
    pub fn is_empty(&self) -> bool {
        self.contracts.is_empty()
    }

    /// Extend the inner set of contracts with the given iterator
    pub fn extend(&mut self, iter: impl IntoIterator<Item = (ContractInfo, DualCompiledContract)>) {
        self.contracts.extend(iter);
    }

    /// Populate the target's factory deps based on the new list
    pub fn extend_factory_deps_by_hash(
        &self,
        mut target: DualCompiledContract,
        factory_deps: impl IntoIterator<Item = H256>,
    ) -> DualCompiledContract {
        let deps_bytecodes = factory_deps
            .into_iter()
            .flat_map(|hash| self.find_by_zk_bytecode_hash(hash))
            .map(|(_, contract)| contract.zk_deployed_bytecode.clone());

        target.zk_factory_deps.extend(deps_bytecodes);
        target
    }

    /// Populate the target's factory deps based on the new list
    ///
    /// Will return `None` if no matching `target` exists
    /// Will not override existing factory deps
    pub fn insert_factory_deps(
        &mut self,
        target: &ContractInfo,
        factory_deps: impl IntoIterator<Item = Vec<u8>>,
    ) -> Option<&DualCompiledContract> {
        self.contracts.get_mut(target).map(|contract| {
            contract.zk_factory_deps.extend(factory_deps);
            &*contract
        })
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::Bytes;
    use zksync_types::bytecode::BytecodeHash;

    use super::*;

    fn find_sample() -> DualCompiledContracts {
        let evm_empty_bytes = Bytes::from_static(&[0]).to_vec();
        let zk_empty_bytes = vec![0u8; 32];

        let zk_bytecode_hash = BytecodeHash::for_bytecode(&zk_empty_bytes).value();

        let sample_contract = DualCompiledContract {
            zk_bytecode_hash,
            zk_deployed_bytecode: zk_empty_bytes,
            zk_factory_deps: Default::default(),
            evm_bytecode_hash: B256::from_slice(&keccak256(&evm_empty_bytes)[..]),
            evm_deployed_bytecode: evm_empty_bytes.clone(),
            evm_bytecode: evm_empty_bytes,
        };

        let infos = [
            ContractInfo::new("src/Foo.sol:Foo"),
            ContractInfo::new("src/Foo.sol:DoubleFoo"),
            ContractInfo::new("test/Foo.t.sol:FooTest"),
            ContractInfo::new("Bar"),
            ContractInfo::new("BarScript"),
            ContractInfo::new("script/Qux.sol:Foo"),
            ContractInfo::new("script/Qux.sol:QuxScript"),
        ];

        let contracts = infos.into_iter().map(|info| (info, sample_contract.clone()));
        DualCompiledContracts {
            contracts: contracts.collect(),
            zk_artifact_path: PathBuf::from("zkout"),
            evm_artifact_path: PathBuf::from("out"),
        }
    }

    #[track_caller]
    fn assert_find_results<'a>(
        results: impl Iterator<Item = (FindMatchType, &'a DualCompiledContract)>,
        assertions: Vec<FindMatchType>,
    ) {
        let results = results.collect::<Vec<_>>();

        let num_assertions = assertions.len();
        let num_results = results.len();
        assert!(
            num_assertions == num_results,
            "unexpected number of results! Expected: {num_assertions}, got: {num_results}"
        );

        for (i, (assertion, (result, _))) in assertions.into_iter().zip(results).enumerate() {
            assert!(
                assertion == result,
                "assertion failed for match #{i}! Expected: {assertion:?}, got: {result:?}"
            );
        }
    }

    #[test]
    fn find_nothing() {
        let collection = find_sample();

        assert_find_results(collection.find(None, None), vec![]);
    }

    #[test]
    fn find_by_full_match() {
        let collection = find_sample();

        let foo_find_asserts = vec![
            FindMatchType::FullMatch,
            FindMatchType::Path,
            // DoubleFoo
            FindMatchType::Path,
            FindMatchType::Name,
            // Qux.sol:Foo
            FindMatchType::Name,
        ];
        assert_find_results(
            collection.find(Some("src/Foo.sol"), Some("Foo")),
            foo_find_asserts.clone(),
        );
        assert_find_results(collection.find(Some("Foo.sol"), Some("Foo")), foo_find_asserts);

        let foo_test_find_asserts =
            vec![FindMatchType::FullMatch, FindMatchType::Path, FindMatchType::Name];
        assert_find_results(
            collection.find(Some("test/Foo.t.sol"), Some("FooTest")),
            foo_test_find_asserts.clone(),
        );
        assert_find_results(
            collection.find(Some("Foo.t.sol"), Some("FooTest")),
            foo_test_find_asserts,
        );
    }

    #[test]
    fn find_by_path() {
        let collection = find_sample();

        let foo_find_asserts = vec![FindMatchType::Path, FindMatchType::Path];
        assert_find_results(collection.find(Some("src/Foo.sol"), None), foo_find_asserts.clone());
        assert_find_results(collection.find(Some("Foo.sol"), None), foo_find_asserts);

        assert_find_results(
            collection.find(Some("test/Foo.t.sol"), None),
            vec![FindMatchType::Path],
        );
        assert_find_results(
            collection.find(Some("Foo.t.sol"), Some("FooTester")),
            vec![FindMatchType::Path],
        );
    }

    #[test]
    fn find_by_name() {
        let collection = find_sample();

        assert_find_results(
            collection.find(None, Some("Foo")),
            vec![FindMatchType::Name, FindMatchType::Name],
        );
        assert_find_results(collection.find(None, Some("QuxScript")), vec![FindMatchType::Name]);

        assert_find_results(collection.find(None, Some("BarScript")), vec![FindMatchType::Name]);
        assert_find_results(
            collection.find(Some("Bar.s.sol"), Some("BarScript")),
            vec![FindMatchType::Name],
        );
    }
}
