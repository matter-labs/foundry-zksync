//! Contract related types.
use crate::artifacts::EraVM;
use alloy_json_abi::JsonAbi;
use foundry_compilers_artifacts_solc::{
    Bytecode, CompactBytecode, CompactContractBytecode, CompactContractBytecodeCow,
    CompactContractRef, CompactDeployedBytecode, DevDoc, Evm, Offsets, StorageLayout, UserDoc,
};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashSet},
};

/// zksolc: Binary object format.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "String", into = "String")]
pub enum ObjectFormat {
    /// Linked
    #[default]
    Raw,
    /// Unlinked
    Elf,
}

impl From<ObjectFormat> for String {
    fn from(val: ObjectFormat) -> Self {
        match val {
            ObjectFormat::Raw => "raw",
            ObjectFormat::Elf => "elf",
        }
        .to_string()
    }
}

impl TryFrom<String> for ObjectFormat {
    type Error = String;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.as_str() {
            "raw" => Ok(Self::Raw),
            "elf" => Ok(Self::Elf),
            s => Err(format!("Unknown zksolc object format: {s}")),
        }
    }
}

impl ObjectFormat {
    /// Returns true if the object format is considered `unlinked`
    pub fn is_unlinked(&self) -> bool {
        matches!(self, Self::Elf)
    }
}

/// Represents a compiled solidity contract
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    /// The contract abi
    pub abi: Option<JsonAbi>,
    /// The contract metadata
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// The contract userdoc
    #[serde(default)]
    pub userdoc: UserDoc,
    /// The contract devdoc
    #[serde(default)]
    pub devdoc: DevDoc,
    /// The contract optimized IR code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ir_optimized: Option<String>,
    /// The contract storage layout.
    #[serde(default, skip_serializing_if = "storage_layout_is_empty")]
    pub storage_layout: StorageLayout,
    /// The contract EraVM bytecode hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    /// Map of factory dependencies, encoded as <hash> => <path>:<name>
    ///
    /// Only contains fully linked factory dependencies, as
    /// unlinked factory dependencies do not have a bytecode hash
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factory_dependencies: Option<BTreeMap<String, String>>,
    /// Complete list of factory dependencies, encoded as <path>:<name>
    ///
    /// Contains both linked and unlinked factory dependencies
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factory_dependencies_unlinked: Option<HashSet<String>>,
    /// EraVM-related outputs
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eravm: Option<EraVM>,
    /// EVM-related outputs (deprecated)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evm: Option<Evm>,
    /// The contract's unlinked libraries
    #[serde(default)]
    pub missing_libraries: Vec<String>,
    /// zksolc's binary object format
    ///
    /// Tells whether the bytecode has been linked.
    ///
    /// Introduced in 1.5.8, beforehand we can assume the bytecode
    /// was always fully linked
    #[serde(default)]
    pub object_format: Option<ObjectFormat>,
}

fn storage_layout_is_empty(storage_layout: &StorageLayout) -> bool {
    storage_layout.storage.is_empty() && storage_layout.types.is_empty()
}

impl Contract {
    /// Returns true if contract is not linked
    pub fn is_unlinked(&self) -> bool {
        self.object_format.as_ref().map(|format| format.is_unlinked()).unwrap_or_default()
    }

    /// takes missing libraries output and transforms into link references
    pub fn missing_libs_to_link_references(
        missing_libraries: &[String],
    ) -> BTreeMap<String, BTreeMap<String, Vec<Offsets>>> {
        missing_libraries
            .iter()
            .map(|file_and_lib| {
                let mut parts = file_and_lib.split(':');
                let filename = parts.next().expect("missing library contract file (<file>:<name>)");
                let contract = parts.next().expect("missing library contract name (<file>:<name>)");
                (filename.to_owned(), contract.to_owned())
            })
            .fold(BTreeMap::default(), |mut acc, (filename, contract)| {
                acc.entry(filename)
                    .or_default()
                    //empty offsets since we can't patch it anyways
                    .insert(contract, vec![]);
                acc
            })
    }

    fn link_references(&self) -> BTreeMap<String, BTreeMap<String, Vec<Offsets>>> {
        Self::missing_libs_to_link_references(self.missing_libraries.as_slice())
    }

    /// Get bytecode
    pub fn bytecode(&self) -> Option<Bytecode> {
        self.eravm
            .as_ref()
            .and_then(|eravm| eravm.bytecode(self.is_unlinked()))
            .or_else(|| {
                self.evm
                    .as_ref()
                    .and_then(|evm| evm.bytecode.as_ref())
                    .map(|bytecode| &bytecode.object)
                    .cloned()
            })
            .map(|object| {
                let mut bytecode: Bytecode = object.into();
                bytecode.link_references = self.link_references();
                bytecode
            })
    }
}

// CompactContract variants
// TODO: for zkEvm, the distinction between bytecode and deployed_bytecode makes little sense,
// and there some fields that the output doesn't provide (e.g: source_map)
// However, we implement these because we get the Artifact trait and can reuse lots of
// the crate's helpers without needing to duplicate everything. Maybe there's a way
// we can get all these without having to add the same bytecode twice on each struct.
// Ideally the Artifacts trait would not be coupled to a specific Contract type
impl<'a> From<&'a Contract> for CompactContractBytecodeCow<'a> {
    fn from(artifact: &'a Contract) -> Self {
        let bc = artifact.bytecode();
        let bytecode = bc.clone().map(|bc| CompactBytecode {
            object: bc.object,
            source_map: None,
            link_references: bc.link_references,
        });
        let deployed_bytecode = bytecode.clone().map(|bytecode| CompactDeployedBytecode {
            bytecode: Some(bytecode),
            immutable_references: Default::default(),
        });

        CompactContractBytecodeCow {
            abi: artifact.abi.as_ref().map(Cow::Borrowed),
            bytecode: bytecode.map(Cow::Owned),
            deployed_bytecode: deployed_bytecode.map(Cow::Owned),
        }
    }
}

impl From<Contract> for CompactContractBytecode {
    fn from(c: Contract) -> Self {
        CompactContractBytecodeCow::from(&c).into()
    }
}

impl<'a> From<&'a Contract> for CompactContractRef<'a> {
    fn from(c: &'a Contract) -> Self {
        let (bin, bin_runtime) = if let Some(ref eravm) = c.eravm {
            (eravm.bytecode.as_ref(), eravm.bytecode.as_ref())
        } else if let Some(ref evm) = c.evm {
            (evm.bytecode.as_ref().map(|c| &c.object), evm.bytecode.as_ref().map(|c| &c.object))
        } else {
            (None, None)
        };

        Self { abi: c.abi.as_ref(), bin, bin_runtime }
    }
}
