use foundry_compilers::{
    artifacts::{output_selection::OutputSelection, Libraries, OptimizerDetails, SettingsMetadata},
    remappings::{self, Remapping},
    EvmVersion,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::Arc,
};

use foundry_compilers::artifacts::{serde_helpers, Source};
const SOLIDITY: &str = "Solidity";

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct ZkSolcConfig {
    /// Path to zksolc binary. Can be a URL.
    pub compiler_path: PathBuf,

    /// zkSolc compiler settings
    pub settings: Settings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remappings: Vec<Remapping>,
    pub optimizer: Optimizer,
    //pub optimizer: Optimizer,
    /// Metadata settings
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<SettingsMetadata>,
    /// This field can be used to select desired outputs based
    /// on file and contract names.
    /// If this field is omitted, then the compiler loads and does type
    /// checking, but will not generate any outputs apart from errors.
    #[serde(default)]
    pub output_selection: OutputSelection,
    /// Addresses of the libraries. If not all libraries are given here,
    /// it can result in unlinked objects whose output data is different.
    ///
    /// The top level key is the name of the source file where the library is used.
    /// If remappings are used, this source file should match the global path
    /// after remappings were applied.
    /// If this key is an empty string, that refers to a global level.
    #[serde(default)]
    pub libraries: Libraries,
    // pub libraries_path: Option<String>,
    pub is_system: bool,
    pub force_evmla: bool,
    // pub contracts_to_compile: Option<Vec<String>>,
}

impl Settings {
    pub fn new(
        remappings: Vec<Remapping>,
        optimizer: Optimizer,
        metadata: Option<SettingsMetadata>,
        output_selection: OutputSelection,
        libraries: Libraries,
        is_system: bool,
        force_evmla: bool,
        // libraries_path: Option<String>,
        // contracts_to_compile: Option<Vec<String>>,
    ) -> Self {
        Self {
            remappings,
            optimizer,
            metadata,
            output_selection,
            libraries,
            is_system,
            force_evmla,
            // libraries_path,
            // contracts_to_compile,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            remappings: Default::default(),
            optimizer: Default::default(),
            metadata: None,
            output_selection: OutputSelection::default_output_selection(),
            libraries: Default::default(),
            is_system: false,
            force_evmla: false,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct Optimizer {
    /// Whether the optimizer is enabled.
    pub enabled: Option<bool>,
    /// The optimization mode string.
    pub mode: Option<String>,
    /// The `solc` optimizer details.
    pub details: Option<OptimizerDetails>,
    /// Whether to try to recompile with -Oz if the bytecode is too large.
    #[serde(rename = "fallbackToOptimizingForSize")]
    pub fallback_to_optimizing_for_size: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct MetadataSettings {
    pub bytecode_hash: Option<String>,
}
#[derive(Default)]
pub struct ZkSolcConfigBuilder {
    compiler_path: PathBuf,
    settings: Option<Settings>,
}

impl ZkSolcConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn compiler_path(mut self, path: PathBuf) -> Self {
        self.compiler_path = path;
        self
    }

    pub fn settings(mut self, settings: Settings) -> Self {
        self.settings = Some(settings);
        self
    }

    pub fn build(self) -> Result<ZkSolcConfig, String> {
        let mut settings = self.settings.unwrap_or_default();
        Ok(ZkSolcConfig { compiler_path: self.compiler_path, settings })
    }
}

/// A `CompilerInput` representation used for verify
///
/// This type is an alternative `CompilerInput` but uses non-alphabetic ordering of the `sources`
/// and instead emits the (Path -> Source) path in the same order as the pairs in the `sources`
/// `Vec`. This is used over a map, so we can determine the order in which etherscan will display
/// the verified contracts
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZkStandardJsonCompilerInput {
    pub language: String,
    #[serde(with = "serde_helpers::tuple_vec_map")]
    pub sources: Vec<(PathBuf, Source)>,
    pub settings: Settings,
}

// === impl StandardJsonCompilerInput ===

impl ZkStandardJsonCompilerInput {
    pub fn new(sources: Vec<(PathBuf, Source)>, settings: Settings) -> Self {
        Self { language: SOLIDITY.to_string(), sources, settings }
    }
}
