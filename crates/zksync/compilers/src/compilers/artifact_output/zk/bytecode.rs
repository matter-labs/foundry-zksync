use std::collections::BTreeMap;

use crate::artifacts::contract::Contract;
use alloy_primitives::Bytes;
use foundry_compilers_artifacts_solc::{
    BytecodeObject, CompactBytecode, CompactDeployedBytecode, Offsets,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct ZkArtifactBytecode {
    object: Bytes,
    is_unlinked: bool,

    #[serde(default)]
    pub missing_libraries: Vec<String>,
}

impl ZkArtifactBytecode {
    pub fn with_object(
        object: BytecodeObject,
        is_unlinked: bool,
        missing_libraries: Vec<String>,
    ) -> Self {
        let object = match object {
            BytecodeObject::Bytecode(bc) => bc,
            BytecodeObject::Unlinked(s) => {
                alloy_primitives::hex::decode(s).expect("valid bytecode").into()
            }
        };
        Self { object, is_unlinked, missing_libraries }
    }

    fn link_references(&self) -> BTreeMap<String, BTreeMap<String, Vec<Offsets>>> {
        Contract::missing_libs_to_link_references(self.missing_libraries.as_slice())
    }

    pub fn object(&self) -> BytecodeObject {
        if self.is_unlinked {
            // convert to unlinked
            let encoded = alloy_primitives::hex::encode(&self.object);
            BytecodeObject::Unlinked(encoded)
        } else {
            // convert to linked
            BytecodeObject::Bytecode(self.object.clone())
        }
    }
}

// NOTE: distinction between bytecode and deployed bytecode makes no sense of zkEvm, but
// we implement these conversions in order to be able to use the Artifacts trait.
impl From<ZkArtifactBytecode> for CompactBytecode {
    fn from(bcode: ZkArtifactBytecode) -> Self {
        let link_references = bcode.link_references();
        Self { object: bcode.object(), source_map: None, link_references }
    }
}

impl From<ZkArtifactBytecode> for CompactDeployedBytecode {
    fn from(bcode: ZkArtifactBytecode) -> Self {
        Self { bytecode: Some(bcode.into()), immutable_references: BTreeMap::default() }
    }
}
