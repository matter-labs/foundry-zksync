//! zksolc settings
use crate::artifacts::output_selection::OutputSelection as ZkOutputSelection;
use foundry_compilers::{
    artifacts::{
        output_selection::OutputSelection, serde_helpers, EvmVersion, Libraries, Remapping,
    },
    compilers::CompilerSettings,
    error::Result,
    solc, CompilerSettingsRestrictions,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashSet},
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
};

use super::{
    types::{ErrorType, WarningType},
    ZkSolc,
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
    /// Solidity remappings
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remappings: Vec<Remapping>,
    #[serde(
        default,
        with = "serde_helpers::display_from_str_opt",
        skip_serializing_if = "Option::is_none"
    )]
    /// EVM version
    pub evm_version: Option<EvmVersion>,

    // check if the same (and use `compilers version`)
    /// This field can be used to select desired outputs based
    /// on file and contract names.
    /// If this field is omitted, then the compiler loads and does type
    /// checking, but will not generate any outputs apart from errors.
    #[serde(default)]
    pub output_selection: ZkOutputSelection,

    #[serde(default)]
    /// Optimizer options
    pub optimizer: Optimizer,
    /// Metadata settings
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<SettingsMetadata>,
    #[serde(default)]
    /// Libraries
    pub libraries: Libraries,
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
    pub suppressed_warnings: HashSet<WarningType>,
    /// Suppressed `zksolc` errors.
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub suppressed_errors: HashSet<ErrorType>,
}

/// Analogous to SolcSettings for zksolc compiler
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ZkSolcSettings {
    /// JSON settings expected by Solc
    #[serde(flatten)]
    pub settings: ZkSettings,
    /// Additional CLI args configuration
    #[serde(flatten)]
    pub cli_settings: solc::CliSettings,
    /// The version of the zksolc compiler to use. Retrieved from `zksolc_path`
    zksolc_version: Version,
    /// zksolc path
    zksolc_path: PathBuf,
}

impl Default for ZkSolcSettings {
    fn default() -> Self {
        let version = ZkSolc::zksolc_latest_supported_version();
        let zksolc_path = ZkSolc::get_path_for_version(&version)
            .expect("failed getting default zksolc version path");
        Self {
            settings: Default::default(),
            cli_settings: Default::default(),
            zksolc_version: version,
            zksolc_path,
        }
    }
}

impl ZkSolcSettings {
    /// Initialize settings for a given zksolc path
    pub fn new_from_path(
        settings: ZkSettings,
        cli_settings: solc::CliSettings,
        zksolc_path: PathBuf,
    ) -> Result<Self> {
        let zksolc_version = ZkSolc::get_version_for_path(&zksolc_path)?;
        Ok(Self { settings, cli_settings, zksolc_path, zksolc_version })
    }

    /// Get zksolc path
    pub fn zksolc_path(&self) -> PathBuf {
        self.zksolc_path.clone()
    }

    /// Get zksolc version
    pub fn zksolc_version_ref(&self) -> &Version {
        &self.zksolc_version
    }

    /// Set a specific zksolc version
    pub fn set_zksolc_version(&mut self, zksolc_version: Version) -> Result<()> {
        let zksolc_path = ZkSolc::get_path_for_version(&zksolc_version)?;
        self.zksolc_version = zksolc_version;
        self.zksolc_path = zksolc_path;
        Ok(())
    }
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

    /// Removes prefix from all paths
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
/// Restrictions for zksolc
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
            *enable_eravm_extensions == other.settings.enable_eravm_extensions &&
            *llvm_options == other.settings.llvm_options &&
            *force_evmla == other.settings.force_evmla &&
            *codegen == other.settings.codegen &&
            *suppressed_warnings == other.settings.suppressed_warnings &&
            *suppressed_errors == other.settings.suppressed_errors &&
            self.zksolc_version == other.zksolc_version
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
/// Optimizer settings
pub struct Optimizer {
    // TODO: does this have to be an option?
    /// Enable the optimizer
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Switch optimizer components on or off in detail.
    /// The "enabled" switch above provides two defaults which can be
    /// tweaked here. If "details" is given, "enabled" can be omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<OptimizerDetails>,
    /// Optimizer mode
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
    /// Disable optimizer
    pub fn disable(&mut self) {
        self.enabled.take();
    }

    /// Enable optimizer
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

/// Optimizer details
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

/// Settings metadata
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsMetadata {
    /// Use the given hash method for the metadata hash that is appended to the bytecode.
    /// The metadata hash can be removed from the bytecode via option "none".
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "serde_helpers::display_from_str_opt"
    )]
    pub hash_type: Option<BytecodeHash>,
    /// hash_type field name for zksolc v1.5.6 and older
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "serde_helpers::display_from_str_opt"
    )]
    bytecode_hash: Option<BytecodeHash>,
}

impl SettingsMetadata {
    /// Creates new SettingsMettadata
    pub fn new(hash_type: Option<BytecodeHash>) -> Self {
        Self { hash_type, bytecode_hash: None }
    }

    /// Makes SettingsMettadata version compatible
    pub fn sanitize(&mut self, zksolc_version: &Version) {
        // zksolc <= 1.5.6 uses "bytecode_hash" field for "hash_type"
        if zksolc_version <= &Version::new(1, 5, 6) {
            self.bytecode_hash = self.hash_type.take();
        }
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
    /// The `ipfs` hash.
    #[serde(rename = "ipfs")]
    Ipfs,
}

impl FromStr for BytecodeHash {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Self::None),
            "ipfs" => Ok(Self::Ipfs),
            "keccak256" => Ok(Self::Keccak256),
            s => Err(format!("Unknown bytecode hash: {s}")),
        }
    }
}

impl fmt::Display for BytecodeHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Keccak256 => "keccak256",
            Self::Ipfs => "ipfs",
            Self::None => "none",
        };
        f.write_str(s)
    }
}
