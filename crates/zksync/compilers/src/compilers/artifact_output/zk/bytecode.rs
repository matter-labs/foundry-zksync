use std::collections::BTreeMap;

use crate::artifacts::contract::{Contract, ObjectFormat};
use alloy_primitives::Bytes;
use foundry_compilers_artifacts_solc::{
    BytecodeObject, CompactBytecode, CompactDeployedBytecode, Offsets,
};
use serde::{Deserialize, Serialize};

/// This will serialize the bytecode data without a `0x` prefix
///
/// Equivalent of solc artifact bytecode's
/// [`serialize_bytecode_without_prefix`](foundry_compilers_artifacts::solc::bytecode::serialize_bytecode_without_prefix)
pub fn serialize_bytes_without_prefix<S>(code: &Bytes, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    s.serialize_str(&alloy_primitives::hex::encode(code))
}

/// Bytecode compiled by zksolc
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct ZkArtifactBytecode {
    #[serde(serialize_with = "serialize_bytes_without_prefix")]
    object: Bytes,
    object_format: ObjectFormat,

    /// Bytecode missing libraries
    #[serde(default)]
    pub missing_libraries: Vec<String>,
}

impl ZkArtifactBytecode {
    /// Get Bytecode from parts
    pub fn with_object(
        object: BytecodeObject,
        object_format: ObjectFormat,
        missing_libraries: Vec<String>,
    ) -> Self {
        let object = match object {
            BytecodeObject::Bytecode(bc) => bc,
            BytecodeObject::Unlinked(s) => {
                alloy_primitives::hex::decode(s).expect("valid bytecode").into()
            }
        };
        Self { object, object_format, missing_libraries }
    }

    /// Returns `true` if the bytecode is unlinked
    pub fn is_unlinked(&self) -> bool {
        self.object_format.is_unlinked()
    }

    /// Get link references
    pub fn link_references(&self) -> BTreeMap<String, BTreeMap<String, Vec<Offsets>>> {
        Contract::missing_libs_to_link_references(self.missing_libraries.as_slice())
    }

    /// Get bytecode object
    pub fn object(&self) -> BytecodeObject {
        if self.object_format.is_unlinked() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialized_bytecode_is_not_prefixed() {
        let object = Bytes::from(vec![0xDEu8, 0xAD, 0xBE, 0xEF]);
        let sample = ZkArtifactBytecode {
            object,
            object_format: ObjectFormat::Raw,
            missing_libraries: vec![],
        };

        let json_str =
            serde_json::to_string(&sample).expect("able to serialize artifact bytecode as json");

        let deserialized: serde_json::Value =
            serde_json::from_str(&json_str).expect("able to deserialize json");

        let bytecode_str = deserialized["object"].as_str().expect(".object to be a string");

        assert!(!bytecode_str.starts_with("0x"));
    }
}
