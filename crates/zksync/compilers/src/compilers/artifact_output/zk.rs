//! ZK Sync artifact output
use crate::artifacts::contract::Contract;
use alloy_json_abi::JsonAbi;
use foundry_compilers::{
    ArtifactOutput,
    artifacts::{
        CompactBytecode, CompactContract, CompactContractBytecode, CompactContractBytecodeCow,
        CompactDeployedBytecode, DevDoc, SourceFile, StorageLayout, UserDoc,
    },
    sources::VersionedSourceFile,
};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashSet},
    path::Path,
};

mod bytecode;
pub use bytecode::ZkArtifactBytecode;

/// Artifact representing a compiled contract
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ZkContractArtifact {
    /// contract abi
    pub abi: Option<JsonAbi>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// contract bytecodee
    pub bytecode: Option<ZkArtifactBytecode>,
    /// contract assembly
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// contract metadata
    pub metadata: Option<serde_json::Value>,
    /// contract storage layout
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_layout: Option<StorageLayout>,
    /// contract userdoc
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub userdoc: Option<UserDoc>,
    /// contract devdoc
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub devdoc: Option<DevDoc>,
    /// contract function hashes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method_identifiers: Option<BTreeMap<String, String>>,
    /// contract evm legacy assembly
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_assembly: Option<serde_json::Value>,
    /// contract optimized IR code
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ir_optimized: Option<String>,
    /// contract hash
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    /// List of factory dependencies, encoded as <hash> => <path>:<name>
    ///
    /// Only contains fully linked factory dependencies
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factory_dependencies: Option<BTreeMap<String, String>>,
    /// Complete list of factory dependencies, encoded as <path>:<name>
    ///
    /// Contains both linked and unlinked factory dependencies
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factory_dependencies_unlinked: Option<HashSet<String>>,
    /// The identifier of the source file
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
}

impl ZkContractArtifact {
    /// Returns a list of _all_ factory deps, by <path>:<name>
    ///
    /// Will return unlinked as well as linked factory deps (might contain duplicates)
    pub fn all_factory_deps(&self) -> impl Iterator<Item = &String> {
        let linked = self.factory_dependencies.iter().flatten().map(|(_, dep)| dep);
        let unlinked = self.factory_dependencies_unlinked.iter().flatten();
        linked.chain(unlinked)
    }

    /// Get contract missing libraries
    pub fn missing_libraries(&self) -> Option<&Vec<String>> {
        self.bytecode.as_ref().map(|bc| &bc.missing_libraries)
    }

    /// Returns true if contract is not linked
    pub fn is_unlinked(&self) -> bool {
        self.bytecode.as_ref().map(|bc| bc.is_unlinked()).unwrap_or(false)
    }
}

// CompactContract variants
// TODO: for zkEvm, the distinction between bytecode and deployed_bytecode makes little sense,
// and there some fields that the output doesn't provide (e.g: source_map)
// However, we implement these because we get the Artifact trait and can reuse lots of
// the crate's helpers without needing to duplicate everything. Maybe there's a way
// we can get all these without having to add the same bytecode twice on each struct.
// Ideally the Artifacts trait would not be coupled to a specific Contract type
impl<'a> From<&'a ZkContractArtifact> for CompactContractBytecodeCow<'a> {
    fn from(artifact: &'a ZkContractArtifact) -> Self {
        // TODO: artifact.abi might have None, we need to get this field from solc_metadata
        CompactContractBytecodeCow {
            abi: artifact.abi.as_ref().map(Cow::Borrowed),
            bytecode: artifact.bytecode.clone().map(|b| Cow::Owned(CompactBytecode::from(b))),
            deployed_bytecode: artifact
                .bytecode
                .clone()
                .map(|b| Cow::Owned(CompactDeployedBytecode::from(b))),
        }
    }
}

impl From<ZkContractArtifact> for CompactContractBytecode {
    fn from(c: ZkContractArtifact) -> Self {
        Self {
            abi: c.abi,
            deployed_bytecode: c.bytecode.clone().map(|b| b.into()),
            bytecode: c.bytecode.clone().map(|b| b.into()),
        }
    }
}

impl From<ZkContractArtifact> for CompactContract {
    fn from(c: ZkContractArtifact) -> Self {
        // TODO: c.abi might have None, we need to get this field from solc_metadata
        Self {
            bin: c.bytecode.clone().map(|b| b.object()),
            bin_runtime: c.bytecode.clone().map(|b| b.object()),
            abi: c.abi,
        }
    }
}

/// ZK Sync ArtifactOutput
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub struct ZkArtifactOutput();

impl ArtifactOutput for ZkArtifactOutput {
    type Artifact = ZkContractArtifact;
    type CompilerContract = Contract;

    fn contract_to_artifact(
        &self,
        _file: &Path,
        _name: &str,
        contract: Self::CompilerContract,
        source_file: Option<&SourceFile>,
    ) -> Self::Artifact {
        let Contract {
            abi,
            metadata,
            userdoc,
            devdoc,
            storage_layout,
            eravm,
            evm,
            ir_optimized,
            hash,
            factory_dependencies,
            factory_dependencies_unlinked,
            missing_libraries,
            object_format,
        } = contract;
        let object_format = object_format.unwrap_or_default();

        let (bytecode, assembly) = eravm
            .map(|eravm| (eravm.bytecode(object_format.is_unlinked()), eravm.assembly))
            .or_else(|| evm.clone().map(|evm| (evm.bytecode.map(|bc| bc.object), evm.assembly)))
            .unwrap_or_else(|| (None, None));
        let bytecode = bytecode.map(|object| {
            ZkArtifactBytecode::with_object(object, object_format, missing_libraries)
        });

        let (method_identifiers, legacy_assembly) = evm
            .map(|evm| (Some(evm.method_identifiers), evm.legacy_assembly))
            .unwrap_or_else(|| (None, None));

        ZkContractArtifact {
            abi,
            hash,
            factory_dependencies,
            factory_dependencies_unlinked,
            storage_layout: Some(storage_layout),
            bytecode,
            assembly,
            metadata,
            method_identifiers,
            legacy_assembly,
            userdoc: Some(userdoc),
            devdoc: Some(devdoc),
            ir_optimized,
            id: source_file.as_ref().map(|s| s.id),
        }
    }

    fn standalone_source_file_to_artifact(
        &self,
        _path: &Path,
        _file: &VersionedSourceFile,
    ) -> Option<Self::Artifact> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::contract::{Contract, ObjectFormat};
    use alloy_json_abi::JsonAbi;
    use alloy_primitives::Bytes;
    use foundry_compilers::artifacts::{Bytecode, BytecodeObject, Evm};
    use std::{
        collections::{BTreeMap, HashSet},
        path::Path,
    };

    #[test]
    fn contract_to_artifact_empty_contract_produces_empty_artifact() {
        let empty_contract = Contract {
            abi: None,
            metadata: None,
            userdoc: UserDoc::default(),
            devdoc: DevDoc::default(),
            ir_optimized: None,
            storage_layout: StorageLayout::default(),
            hash: None,
            factory_dependencies: None,
            factory_dependencies_unlinked: None,
            eravm: None,
            evm: None,
            missing_libraries: Vec::new(),
            object_format: None,
        };

        let artifact =
            ZkArtifactOutput().contract_to_artifact(Path::new(""), "Empty", empty_contract, None);

        let expected = ZkContractArtifact {
            abi: None,
            bytecode: None,
            assembly: None,
            metadata: None,
            storage_layout: Some(StorageLayout::default()),
            userdoc: Some(UserDoc::default()),
            devdoc: Some(DevDoc::default()),
            method_identifiers: None,
            legacy_assembly: None,
            ir_optimized: None,
            hash: None,
            factory_dependencies: None,
            factory_dependencies_unlinked: None,
            id: None,
        };

        assert_eq!(artifact, expected);
    }

    #[test]
    fn contract_to_artifact_simple_valid_fields() {
        let abi = Some(serde_json::from_str::<JsonAbi>(
            r#"[
                {"type":"function","name":"foo","inputs":[],"outputs":[]},
                {"type":"function","name":"bar","inputs":[{"name":"x","type":"uint256"}],"outputs":[]}
            ]"#,
        ).expect("valid abi"));
        let metadata = Some(serde_json::json!({"k":"v"}));
        let userdoc = UserDoc { notice: Some("note".to_string()), ..Default::default() };
        let devdoc = DevDoc { title: Some("title".to_string()), ..Default::default() };
        let storage_layout = StorageLayout::default();
        let hash = Some("0xdeadbeef".to_string());
        let ir_optimized = Some("IR".to_string());
        let factory_dependencies =
            Some(BTreeMap::from([("0x01".to_string(), "src/Lib.sol:Lib".to_string())]));
        let factory_dependencies_unlinked =
            Some(HashSet::from(["src/Another.sol:Another".to_string()]));
        let missing_libraries = vec!["src/Lib.sol:Lib".to_string()];
        let method_identifiers_map = BTreeMap::from([
            ("foo()".to_string(), "11111111".to_string()),
            ("bar(uint256)".to_string(), "22222222".to_string()),
        ]);
        let legacy_asm = serde_json::json!({"foo":"bar"});

        let evm = Some(Evm {
            assembly: Some("ASM".to_string()),
            legacy_assembly: Some(legacy_asm.clone()),
            bytecode: Some(Bytecode {
                function_debug_data: Default::default(),
                object: BytecodeObject::Bytecode(Bytes::from(vec![0x01u8, 0x02])),
                opcodes: None,
                source_map: None,
                generated_sources: vec![],
                link_references: BTreeMap::new(),
            }),
            method_identifiers: method_identifiers_map.clone(),
            deployed_bytecode: None,
            gas_estimates: None,
        });

        let simple_contract = Contract {
            abi: abi.clone(),
            metadata: metadata.clone(),
            userdoc: userdoc.clone(),
            devdoc: devdoc.clone(),
            ir_optimized: ir_optimized.clone(),
            storage_layout: storage_layout.clone(),
            hash: hash.clone(),
            factory_dependencies: factory_dependencies.clone(),
            factory_dependencies_unlinked: factory_dependencies_unlinked.clone(),
            eravm: None,
            evm,
            missing_libraries: missing_libraries.clone(),
            object_format: Some(ObjectFormat::Raw),
        };

        let artifact =
            ZkArtifactOutput().contract_to_artifact(Path::new(""), "Simple", simple_contract, None);

        let expected_bytecode = Some(ZkArtifactBytecode::with_object(
            BytecodeObject::Bytecode(Bytes::from(vec![0x01u8, 0x02])),
            ObjectFormat::Raw,
            missing_libraries,
        ));

        let expected = ZkContractArtifact {
            abi,
            hash,
            factory_dependencies,
            factory_dependencies_unlinked,
            storage_layout: Some(storage_layout),
            bytecode: expected_bytecode,
            assembly: Some("ASM".to_string()),
            metadata,
            method_identifiers: Some(method_identifiers_map),
            legacy_assembly: Some(legacy_asm),
            userdoc: Some(userdoc),
            devdoc: Some(devdoc),
            ir_optimized,
            id: None,
        };

        assert_eq!(artifact, expected);
    }
}
