use crate::artifacts::output_selection::OutputSelection as ZkOutputSelection;
use foundry_compilers::{
    artifacts::{serde_helpers, EvmVersion, Libraries},
    compilers::CompilerSettings,
    solc, CompilerSettingsRestrictions,
};
use foundry_compilers_artifacts_solc::{output_selection::OutputSelection, remappings::Remapping};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashSet},
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
};

///
/// The Solidity compiler codegen.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codegen {
    /// The Yul IR.
    #[default]
    Yul,
    /// The EVM legacy assembly IR.
    EVMLA,
}

/// `zksolc` warnings that can be suppressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ZkSolcWarning {
    /// `txorigin` warning: Using `tx.origin` in place of `msg.sender`.
    TxOrigin,
}

impl FromStr for ZkSolcWarning {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "txorigin" => Ok(Self::TxOrigin),
            s => Err(format!("Unknown zksolc warning: {s}")),
        }
    }
}

/// `zksolc` errors that can be suppressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ZkSolcError {
    /// `sendtransfer` error: Using `send()` or `transfer()` methods on `address payable` instead
    /// of `call()`.
    SendTransfer,
}

impl FromStr for ZkSolcError {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sendtransfer" => Ok(Self::SendTransfer),
            s => Err(format!("Unknown zksolc error: {s}")),
        }
    }
}

/// zksolc standard json input settings. See:
/// https://docs.zksync.io/zk-stack/components/compiler/toolchain/solidity.html#standard-json for differences
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZkSettings {
    // same
    /// Change compilation pipeline to go through the Yul intermediate representation. This is
    /// false by default.
    #[serde(rename = "viaIR", default, skip_serializing_if = "Option::is_none")]
    pub via_ir: Option<bool>,
    /// The Solidity codegen.
    #[serde(default)]
    pub codegen: Codegen,
    // TODO: era-compiler-solidity uses a BTreeSet of strings. In theory the serialization
    // should be the same but maybe we should double check
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remappings: Vec<Remapping>,
    #[serde(
        default,
        with = "serde_helpers::display_from_str_opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub evm_version: Option<EvmVersion>,

    // check if the same (and use `compilers version`)
    /// This field can be used to select desired outputs based
    /// on file and contract names.
    /// If this field is omitted, then the compiler loads and does type
    /// checking, but will not generate any outputs apart from errors.
    #[serde(default)]
    pub output_selection: ZkOutputSelection,

    #[serde(default)]
    pub optimizer: Optimizer,
    /// Metadata settings
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<SettingsMetadata>,
    #[serde(default)]
    pub libraries: Libraries,
    /// Switch to missing deployable libraries detection mode.
    /// Contracts are not compiled in this mode, and all compilation artifacts are not included.
    #[serde(default, rename = "detectMissingLibraries")]
    pub detect_missing_libraries: bool,
    // zksolc arguments
    /// A flag indicating whether to enable the system contract compilation mode.
    /// Whether to enable EraVM extensions.
    #[serde(default, rename = "enableEraVMExtensions")]
    pub enable_eravm_extensions: bool,
    /// The extra LLVM options.
    #[serde(default, rename = "LLVMOptions", skip_serializing_if = "Vec::is_empty")]
    pub llvm_options: Vec<String>,
    /// Whether to compile via EVM assembly.
    #[serde(default, rename = "forceEVMLA")]
    pub force_evmla: bool,
    /// Suppressed `zksolc` warnings.
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub suppressed_warnings: HashSet<ZkSolcWarning>,
    /// Suppressed `zksolc` errors.
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub suppressed_errors: HashSet<ZkSolcError>,
}

// Analogous to SolcSettings for Zk compiler
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ZkSolcSettings {
    /// JSON settings expected by Solc
    #[serde(flatten)]
    pub settings: ZkSettings,
    /// Additional CLI args configuration
    #[serde(flatten)]
    pub cli_settings: solc::CliSettings,
}

impl ZkSettings {
    /// Creates a new `Settings` instance with the given `output_selection`
    pub fn new(output_selection: impl Into<ZkOutputSelection>) -> Self {
        Self { output_selection: output_selection.into(), ..Default::default() }
    }

    /// Consumes the type and returns a [ZkSettings::sanitize] version
    pub fn sanitized(mut self, solc_version: &Version) -> Self {
        self.sanitize(solc_version);
        self
    }

    /// This will remove/adjust values in the settings that are not compatible with this version.
    pub fn sanitize(&mut self, solc_version: &Version) {
        if let Some(ref mut evm_version) = self.evm_version {
            self.evm_version = evm_version.normalize_version_solc(solc_version);
        }
    }

    pub fn strip_prefix(&mut self, base: impl AsRef<Path>) {
        let base = base.as_ref();
        self.remappings.iter_mut().for_each(|r| {
            r.strip_prefix(base);
        });

        self.libraries.libs = std::mem::take(&mut self.libraries.libs)
            .into_iter()
            .map(|(file, libs)| (file.strip_prefix(base).map(Into::into).unwrap_or(file), libs))
            .collect();
    }

    /// Strips `base` from all paths
    pub fn with_base_path(mut self, base: impl AsRef<Path>) -> Self {
        let base = base.as_ref();
        self.remappings.iter_mut().for_each(|r| {
            r.strip_prefix(base);
        });

        self.libraries.libs = self
            .libraries
            .libs
            .into_iter()
            .map(|(file, libs)| (file.strip_prefix(base).map(Into::into).unwrap_or(file), libs))
            .collect();

        self
    }
}

impl Default for ZkSettings {
    fn default() -> Self {
        Self {
            optimizer: Default::default(),
            metadata: None,
            output_selection: Default::default(),
            evm_version: Some(EvmVersion::default()),
            via_ir: None,
            libraries: Default::default(),
            remappings: Default::default(),
            detect_missing_libraries: false,
            enable_eravm_extensions: false,
            llvm_options: Default::default(),
            force_evmla: false,
            codegen: Default::default(),
            suppressed_errors: Default::default(),
            suppressed_warnings: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ZkSolcRestrictions();

impl CompilerSettingsRestrictions for ZkSolcRestrictions {
    fn merge(self, _other: Self) -> Option<Self> {
        None
    }
}

impl CompilerSettings for ZkSolcSettings {
    type Restrictions = ZkSolcRestrictions;

    fn update_output_selection(&mut self, _f: impl FnOnce(&mut OutputSelection) + Copy) {
        // TODO: see how to support this, noop for now
        //f(&mut self.output_selection)
    }

    fn can_use_cached(&self, other: &Self) -> bool {
        let Self {
            settings:
                ZkSettings {
                    via_ir,
                    remappings,
                    evm_version,
                    output_selection,
                    optimizer,
                    metadata,
                    libraries,
                    detect_missing_libraries,
                    enable_eravm_extensions,
                    llvm_options,
                    force_evmla,
                    codegen,
                    suppressed_warnings,
                    suppressed_errors,
                },
            ..
        } = self;

        *via_ir == other.settings.via_ir &&
            *remappings == other.settings.remappings &&
            *evm_version == other.settings.evm_version &&
            *output_selection == other.settings.output_selection &&
            *optimizer == other.settings.optimizer &&
            *metadata == other.settings.metadata &&
            *libraries == other.settings.libraries &&
            *detect_missing_libraries == other.settings.detect_missing_libraries &&
            *enable_eravm_extensions == other.settings.enable_eravm_extensions &&
            *llvm_options == other.settings.llvm_options &&
            *force_evmla == other.settings.force_evmla &&
            *codegen == other.settings.codegen &&
            *suppressed_warnings == other.settings.suppressed_warnings &&
            *suppressed_errors == other.settings.suppressed_errors
    }

    fn with_remappings(mut self, remappings: &[Remapping]) -> Self {
        self.settings.remappings = remappings.to_vec();

        self
    }

    fn with_allow_paths(mut self, allowed_paths: &BTreeSet<PathBuf>) -> Self {
        self.cli_settings.allow_paths.clone_from(allowed_paths);
        self
    }

    fn with_base_path(mut self, base_path: &Path) -> Self {
        self.cli_settings.base_path = Some(base_path.to_path_buf());
        self
    }

    fn with_include_paths(mut self, include_paths: &BTreeSet<PathBuf>) -> Self {
        self.cli_settings.include_paths.clone_from(include_paths);
        self
    }

    fn satisfies_restrictions(&self, _restrictions: &Self::Restrictions) -> bool {
        // TODO
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Optimizer {
    // TODO: does this have to be an option?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Switch optimizer components on or off in detail.
    /// The "enabled" switch above provides two defaults which can be
    /// tweaked here. If "details" is given, "enabled" can be omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<OptimizerDetails>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<char>,
    /// Whether to try to recompile with -Oz if the bytecode is too large.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_to_optimizing_for_size: Option<bool>,
    /// Whether to disable the system request memoization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_system_request_memoization: Option<bool>,
    /// Set the jump table density threshold.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jump_table_density_threshold: Option<u32>,
}

impl Optimizer {
    pub fn disable(&mut self) {
        self.enabled.take();
    }

    pub fn enable(&mut self) {
        self.enabled = Some(true)
    }
}

impl Default for Optimizer {
    fn default() -> Self {
        Self {
            enabled: Some(false),
            mode: None,
            fallback_to_optimizing_for_size: None,
            disable_system_request_memoization: None,
            jump_table_density_threshold: None,
            details: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OptimizerDetails {
    /// The peephole optimizer is always on if no details are given,
    /// use details to switch it off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peephole: Option<bool>,
    /// The inliner is always on if no details are given,
    /// use details to switch it off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inliner: Option<bool>,
    /// The unused jumpdest remover is always on if no details are given,
    /// use details to switch it off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jumpdest_remover: Option<bool>,
    /// Sometimes re-orders literals in commutative operations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_literals: Option<bool>,
    /// Removes duplicate code blocks
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deduplicate: Option<bool>,
    /// Common subexpression elimination, this is the most complicated step but
    /// can also provide the largest gain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cse: Option<bool>,
    /// Optimize representation of literal numbers and strings in code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constant_optimizer: Option<bool>,
}

impl OptimizerDetails {
    /// Returns true if no settings are set.
    pub fn is_empty(&self) -> bool {
        self.peephole.is_none() &&
            self.inliner.is_none() &&
            self.jumpdest_remover.is_none() &&
            self.order_literals.is_none() &&
            self.deduplicate.is_none() &&
            self.cse.is_none() &&
            self.constant_optimizer.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsMetadata {
    /// Use the given hash method for the metadata hash that is appended to the bytecode.
    /// The metadata hash can be removed from the bytecode via option "none".
    /// `zksolc` only supports keccak256
    #[serde(
        default,
        rename = "bytecodeHash",
        skip_serializing_if = "Option::is_none",
        with = "serde_helpers::display_from_str_opt"
    )]
    pub bytecode_hash: Option<BytecodeHash>,
}

impl SettingsMetadata {
    pub fn new(hash: BytecodeHash) -> Self {
        Self { bytecode_hash: Some(hash) }
    }
}

impl From<BytecodeHash> for SettingsMetadata {
    fn from(hash: BytecodeHash) -> Self {
        Self { bytecode_hash: Some(hash) }
    }
}

/// Determines the hash method for the metadata hash that is appended to the bytecode.
/// Zksolc only supports keccak256
#[derive(Clone, Debug, Default, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BytecodeHash {
    /// Do not include bytecode hash.
    #[default]
    #[serde(rename = "none")]
    None,
    /// The default keccak256 hash.
    #[serde(rename = "keccak256")]
    Keccak256,
}

impl FromStr for BytecodeHash {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Self::None),
            "keccak256" => Ok(Self::Keccak256),
            s => Err(format!("Unknown bytecode hash: {s}")),
        }
    }
}

impl fmt::Display for BytecodeHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Keccak256 => "keccak256",
            Self::None => "none",
        };
        f.write_str(s)
    }
}
