//! zksolc output selection
use serde::{Deserialize, Serialize};

use std::collections::HashSet;

///
/// The `solc --standard-json` output selection.
#[derive(Debug, Default, Serialize, Deserialize, Eq, PartialEq, Clone)]
pub struct OutputSelection {
    /// Only the 'all' wildcard is available for robustness reasons.
    #[serde(rename = "*")]
    pub all: FileOutputSelection,
}

/// The `solc --standard-json` expected output selection value.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileOutputSelection {
    /// The per-file output selections.
    #[serde(rename = "")]
    pub per_file: HashSet<OutputSelectionFlag>,
    /// The per-contract output selections.
    #[serde(rename = "*")]
    pub per_contract: HashSet<OutputSelectionFlag>,
}

///
/// The `solc --standard-json` expected output selection flag.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputSelectionFlag {
    /// The ABI JSON.
    #[serde(rename = "abi")]
    ABI,
    /// The metadata.
    #[serde(rename = "metadata")]
    Metadata,
    /// The developer documentation.
    #[serde(rename = "devdoc")]
    Devdoc,
    /// The user documentation.
    #[serde(rename = "userdoc")]
    Userdoc,
    /// The function signature hashes JSON.
    #[serde(rename = "evm.methodIdentifiers")]
    MethodIdentifiers,
    /// The storage layout.
    #[serde(rename = "storageLayout")]
    StorageLayout,
    /// The AST JSON.
    #[serde(rename = "ast")]
    AST,
    /// The Yul IR.
    #[serde(rename = "irOptimized")]
    Yul,
    /// The EVM legacy assembly JSON.
    #[serde(rename = "evm.legacyAssembly")]
    EVMLA,
}

impl std::fmt::Display for OutputSelectionFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ABI => write!(f, "abi"),
            Self::Metadata => write!(f, "metadata"),
            Self::Devdoc => write!(f, "devdoc"),
            Self::Userdoc => write!(f, "userdoc"),
            Self::MethodIdentifiers => write!(f, "evm.methodIdentifiers"),
            Self::StorageLayout => write!(f, "storageLayout"),
            Self::AST => write!(f, "ast"),
            Self::Yul => write!(f, "irOptimized"),
            Self::EVMLA => write!(f, "evm.legacyAssembly"),
        }
    }
}
