//! zkSolc Compiler Configuration Module.
//!
//! This module defines structures and builders for configuring the zkSolc compiler.
//! It includes settings for the compiler path, various compiler options, optimization settings,
//! and other parameters that influence how Solidity code is compiled using zkSolc.
//!
//! The main structures in this module are `ZkSolcConfig`, which holds the overall configuration,
//! and `Settings`, which encapsulate specific compiler settings. Additionally, `Optimizer` provides
//! detailed settings for bytecode optimization.
//!
//! This module also provides a builder pattern implementation (`ZkSolcConfigBuilder`) for
//! constructing a `ZkSolcConfig` instance in a flexible and convenient manner.
use foundry_compilers::{
    artifacts::{
        output_selection::OutputSelection, serde_helpers, Libraries, OptimizerDetails,
        SettingsMetadata, Source,
    },
    remappings::Remapping,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::setup_zksolc_manager;

const SOLIDITY: &str = "Solidity";
/// Configuration for the zkSolc compiler.
///
/// This struct holds the configuration settings used for the zkSolc compiler,
/// including the path to the compiler binary and various compiler settings.
#[derive(Clone, Debug, Default)]
pub struct ZkSolcConfig {
    /// Path to zksolc binary. Can be a URL.
    pub compiler_path: PathBuf,

    /// zkSolc compiler settings
    pub settings: Settings,

    /// contracts to compile
    pub contracts_to_compile: Option<Vec<globset::GlobMatcher>>,

    /// contracts to avoid compiling
    pub avoid_contracts: Option<Vec<globset::GlobMatcher>>,
}

/// Compiler settings for zkSolc.
///
/// This struct holds various settings that influence the behavior of the zkSolc compiler.
/// These settings include file remappings, optimization options, metadata settings,
/// output selection criteria, library addresses, and flags for specific compilation modes.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// A list of remappings to apply to the source files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remappings: Vec<Remapping>,
    /// The `zksolc` optimization settings.
    pub optimizer: Optimizer,
    /// Metadata settings
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<SettingsMetadata>,
    /// This field can be used to select desired outputs based
    /// on file and contract names.
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
    /// A flag indicating whether to enable the system contract compilation mode.
    pub is_system: bool,
    /// A flag indicating whether to forcibly switch to the EVM legacy assembly pipeline.
    pub force_evmla: bool,
    /// Path to cache missing library dependencies, used for compiling and deploying libraries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_libraries_path: Option<String>,
    /// Flag to indicate if there are missing libraries, used to enable/disable logs for successful
    /// compilation.
    #[serde(default)]
    pub are_libraries_missing: bool,
    /// List of specific contracts to be compiled.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contracts_to_compile: Vec<String>,
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
            missing_libraries_path: None,
            are_libraries_missing: false,
            contracts_to_compile: Default::default(),
        }
    }
}

/// Creates [Settings].
#[derive(Default)]
pub struct SettingsBuilder {
    remappings: Vec<Remapping>,
    optimizer: OptimizerBuilder,
    metadata: Option<SettingsMetadata>,
    output_selection: OutputSelection,
    libraries: Libraries,
    is_system: bool,
    force_evmla: bool,
    missing_libraries_path: Option<String>,
    are_libraries_missing: bool,
    contracts_to_compile: Vec<String>,
}

impl SettingsBuilder {
    /// Creates a new instance of [SettingsBuilder].
    pub fn new() -> Self {
        Default::default()
    }

    /// Sets remappings.
    pub fn remappings(mut self, value: Vec<Remapping>) -> Self {
        self.remappings = value;
        self
    }

    /// Sets optimizer settings via a builder.
    pub fn optimizer<F>(mut self, builder_fn: F) -> Self
    where
        F: FnOnce(OptimizerBuilder) -> OptimizerBuilder,
    {
        self.optimizer = builder_fn(self.optimizer);
        self
    }

    /// Sets metadata.
    pub fn metadata(mut self, value: Option<SettingsMetadata>) -> Self {
        self.metadata = value;
        self
    }

    /// Sets output_selection.
    pub fn output_selection(mut self, value: OutputSelection) -> Self {
        self.output_selection = value;
        self
    }

    /// Sets libraries.
    pub fn libraries(mut self, value: Libraries) -> Self {
        self.libraries = value;
        self
    }

    /// Sets is_system.
    pub fn is_system(mut self, value: bool) -> Self {
        self.is_system = value;
        self
    }

    /// Sets force_evmla.
    pub fn force_evmla(mut self, value: bool) -> Self {
        self.force_evmla = value;
        self
    }

    /// Sets missing_libraries_path.
    pub fn missing_libraries_path(mut self, value: Option<String>) -> Self {
        self.missing_libraries_path = value;
        self
    }

    /// Sets are_libraries_missing.
    pub fn are_libraries_missing(mut self, value: bool) -> Self {
        self.are_libraries_missing = value;
        self
    }

    /// Sets contracts_to_compile.
    pub fn contracts_to_compile(mut self, value: Vec<String>) -> Self {
        self.contracts_to_compile = value;
        self
    }

    /// Builds the [Settings].
    pub fn build(mut self) -> Result<Settings, String> {
        Ok(Settings {
            remappings: self.remappings,
            optimizer: self.optimizer.build()?,
            metadata: self.metadata.take(),
            output_selection: self.output_selection,
            libraries: self.libraries,
            is_system: self.is_system,
            force_evmla: self.force_evmla,
            missing_libraries_path: self.missing_libraries_path.take(),
            are_libraries_missing: self.are_libraries_missing,
            contracts_to_compile: self.contracts_to_compile,
        })
    }
}

/// Settings for the optimizer used in zkSolc compiler.
///
/// This struct configures how the zkSolc compiler optimizes the generated bytecode.
/// It includes settings for enabling the optimizer, choosing the optimization mode,
/// specifying detailed optimization parameters, and handling bytecode size constraints.
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
    /// Whether to disable the system request memoization.
    #[serde(rename = "disableSystemRequestMemoization")]
    pub disable_system_request_memoization: bool,
}

/// A builder for [Optimizer].
#[derive(Default)]
pub struct OptimizerBuilder {
    enabled: Option<bool>,
    mode: Option<String>,
    details: Option<OptimizerDetails>,
    fallback_to_optimizing_for_size: Option<bool>,
    disable_system_request_memoization: bool,
}

impl OptimizerBuilder {
    /// Creates a new [ZkSolcConfigOptimizerBuilder].
    pub fn new() -> Self {
        Default::default()
    }

    /// Sets enabled.
    pub fn enabled(mut self, value: bool) -> Self {
        self.enabled = Some(value);
        self
    }

    /// Sets mode.
    pub fn mode(mut self, mode: String) -> Self {
        self.mode = Some(mode);
        self
    }

    /// Sets details.
    pub fn details(mut self, value: Option<OptimizerDetails>) -> Self {
        self.details = value;
        self
    }

    /// Sets optimize_for_size_fallback.
    pub fn optimize_for_size_fallback(mut self, value: bool) -> Self {
        self.fallback_to_optimizing_for_size = Some(value);
        self
    }
    /// Sets disable_system_request_memoization.
    pub fn disable_system_request_memoization(mut self, value: bool) -> Self {
        self.disable_system_request_memoization = value;
        self
    }

    /// Builds the [Optimizer].
    pub fn build(mut self) -> Result<Optimizer, String> {
        Ok(Optimizer {
            enabled: self.enabled.take(),
            mode: self.mode.take(),
            details: self.details.take(),
            fallback_to_optimizing_for_size: self.fallback_to_optimizing_for_size.take(),
            disable_system_request_memoization: self.disable_system_request_memoization,
        })
    }
}

/// A builder for `ZkSolcConfig`.
#[derive(Default)]
pub struct ZkSolcConfigBuilder {
    compiler_version: Option<semver::Version>,
    compiler_path: Option<PathBuf>,
    contracts_to_compile: Option<Vec<String>>,
    avoid_contracts: Option<Vec<String>>,
    settings: SettingsBuilder,
}

impl ZkSolcConfigBuilder {
    /// Creates a new `ZkSolcConfigBuilder`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the `zksolc` version.
    pub fn compiler_version(mut self, version: semver::Version) -> Self {
        self.compiler_version = Some(version);
        self
    }

    /// Sets the path to the `zksolc` binary.
    pub fn compiler_path(mut self, path: PathBuf) -> Self {
        self.compiler_path = Some(path);
        self
    }

    /// Sets the `Settings` for the `ZkSolcConfig`.
    pub fn settings<F>(mut self, builder_fn: F) -> Self
    where
        F: FnOnce(SettingsBuilder) -> SettingsBuilder,
    {
        self.settings = builder_fn(self.settings);
        self
    }

    /// Sets contracts_to_compile.
    pub fn contracts_to_compile(mut self, value: Option<Vec<String>>) -> Self {
        self.contracts_to_compile = value;
        self
    }

    /// Sets avoid_contracts.
    pub fn avoid_contracts(mut self, value: Option<Vec<String>>) -> Self {
        self.avoid_contracts = value;
        self
    }

    /// Builds the `ZkSolcConfig`.
    pub fn build(self) -> Result<ZkSolcConfig, String> {
        let settings = self.settings.build()?;
        let compiler_path = if let Some(compiler_path) = self.compiler_path {
            compiler_path
        } else {
            // TODO: we are forcibly converting this method to sync since it can be called either
            // within a sync (tests) or async (binary) context. We should fix that and stick to
            // a single context
            match tokio::runtime::Handle::try_current() {
                Ok(handle) => std::thread::spawn(move || {
                    handle
                        .block_on(setup_zksolc_manager(self.compiler_version))
                        .map_err(|err| err.to_string())
                })
                .join()
                .map_err(|err| format!("{err:?}"))?,
                Err(_) => tokio::runtime::Runtime::new()
                    .expect("failed starting runtime")
                    .block_on(setup_zksolc_manager(self.compiler_version))
                    .map_err(|err| err.to_string()),
            }
            .map_err(|err| format!("failed setting up zksolc: {err}"))?
        };

        Ok(ZkSolcConfig {
            compiler_path,
            settings,
            contracts_to_compile: self.contracts_to_compile.map(|patterns| {
                patterns
                    .into_iter()
                    .map(|pat| globset::Glob::new(&pat).expect("invalid pattern").compile_matcher())
                    .collect::<Vec<_>>()
            }),
            avoid_contracts: self.avoid_contracts.map(|patterns| {
                patterns
                    .into_iter()
                    .map(|pat| globset::Glob::new(&pat).expect("invalid pattern").compile_matcher())
                    .collect::<Vec<_>>()
            }),
        })
    }
}

/// A `ZkStandardJsonCompilerInput` representation used for verify
///
/// This type is an alternative `ZkStandardJsonCompilerInput` but uses non-alphabetic ordering of
/// the `sources` and instead emits the (Path -> Source) path in the same order as the pairs in the
/// `sources` `Vec`. This is used over a map, so we can determine the order in which etherscan will
/// display the verified contracts
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZkStandardJsonCompilerInput {
    /// The language used in the source files.
    pub language: String,
    /// A map of source file names to their corresponding source code.
    #[serde(with = "serde_helpers::tuple_vec_map")]
    pub sources: Vec<(PathBuf, Source)>,
    /// The zksolc compiler settings.
    pub settings: Settings,
}
impl ZkStandardJsonCompilerInput {
    /// Creates a new `ZkStandardJsonCompilerInput` instance with the specified parameters.
    pub fn new(sources: Vec<(PathBuf, Source)>, settings: Settings) -> Self {
        Self { language: SOLIDITY.to_string(), sources, settings }
    }
}
