//! Contract related types.
use crate::artifacts::EraVM;
use alloy_json_abi::JsonAbi;
use foundry_compilers_artifacts_solc::{
    Bytecode, CompactBytecode, CompactContractBytecode, CompactContractBytecodeCow,
    CompactContractRef, CompactDeployedBytecode, DevDoc, Offsets, StorageLayout, UserDoc,
};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::BTreeMap};

/// Represents a compiled solidity contract
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    pub abi: Option<JsonAbi>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub userdoc: UserDoc,
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
    /// The contract factory dependencies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factory_dependencies: Option<BTreeMap<String, String>>,
    /// EVM-related outputs
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eravm: Option<EraVM>,
    /// The contract's unlinked libraries
    #[serde(default)]
    pub missing_libraries: Vec<String>,
}

fn storage_layout_is_empty(storage_layout: &StorageLayout) -> bool {
    storage_layout.storage.is_empty() && storage_layout.types.is_empty()
}

impl Contract {
    pub fn is_unlinked(&self) -> bool {
        self.hash.is_none() || !self.missing_libraries.is_empty()
    }

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

    pub fn bytecode(&self) -> Option<Bytecode> {
        self.eravm.as_ref().and_then(|eravm| eravm.bytecode(self.is_unlinked())).map(|object| {
            let mut bytecode: Bytecode = object.into();
            bytecode.link_references = self.link_references();
            bytecode
        })
    }
}

// CompactContract variants
// TODO: for zkEvm, the distinction between bytecode and deployed_bytecode makes little sense,
// and there some fields that the ouptut doesn't provide (e.g: source_map)
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
        } else {
            (None, None)
        };

        Self { abi: c.abi.as_ref(), bin, bin_runtime }
    }
}
