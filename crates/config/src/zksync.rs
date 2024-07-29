use foundry_compilers::{
    artifacts::{
        zksolc::output_selection::{FileOutputSelection, OutputSelection, OutputSelectionFlag},
        EvmVersion, Libraries,
    },
    zksolc::settings::{
        BytecodeHash, Optimizer, OptimizerDetails, SettingsMetadata, ZkSolcSettings,
    },
};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::SolcReq;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// ZkSync configuration
pub struct ZkSyncConfig {
    /// Compile for zkVM
    pub compile: bool,

    /// Start VM in zkVM mode
    pub startup: bool,

    /// The zkSolc instance to use if any.
    pub zksolc: Option<SolcReq>,

    /// solc path to use along the zksolc compiler
    pub solc_path: Option<PathBuf>,

    /// Whether to include the metadata hash for zksolc compiled bytecode.
    pub bytecode_hash: Option<BytecodeHash>,

    /// Whether to try to recompile with -Oz if the bytecode is too large.
    pub fallback_oz: bool,

    /// Whether to support compilation of zkSync-specific simulations
    pub enable_eravm_extensions: bool,

    /// Force evmla for zkSync
    pub force_evmla: bool,

    pub llvm_options: Vec<String>,
    /// Detect missing libraries, instead of erroring
    ///
    /// Currently unused
    pub detect_missing_libraries: bool,

    /// Source files to avoid compiling on zksolc
    pub avoid_contracts: Option<Vec<String>>,

    /// Enable optimizer for zkSync
    pub optimizer: bool,

    /// The optimization mode string for zkSync
    pub optimizer_mode: char,

    /// zkSolc optimizer details
    pub optimizer_details: Option<OptimizerDetails>,
}

impl Default for ZkSyncConfig {
    fn default() -> Self {
        Self {
            compile: Default::default(),
            startup: true,
            zksolc: Default::default(),
            solc_path: Default::default(),
            bytecode_hash: Default::default(),
            fallback_oz: Default::default(),
            enable_eravm_extensions: Default::default(),
            force_evmla: Default::default(),
            detect_missing_libraries: Default::default(),
            llvm_options: Default::default(),
            avoid_contracts: Default::default(),
            optimizer: true,
            optimizer_mode: '3',
            optimizer_details: Default::default(),
        }
    }
}

impl ZkSyncConfig {
    /// Returns true if zk mode is enabled and it if tests should be run in zk mode
    pub fn run_in_zk_mode(&self) -> bool {
        self.compile && self.startup
    }

    /// Returns true if contracts should be compiled for zk
    pub fn should_compile(&self) -> bool {
        self.compile
    }

    /// Convert the zksync config to a foundry_compilers zksync Settings
    pub fn settings(
        &self,
        libraries: Libraries,
        evm_version: EvmVersion,
        via_ir: bool,
    ) -> ZkSolcSettings {
        let optimizer = Optimizer {
            enabled: Some(self.optimizer),
            mode: Some(self.optimizer_mode),
            fallback_to_optimizing_for_size: Some(self.fallback_oz),
            disable_system_request_memoization: Some(true),
            details: self.optimizer_details.clone(),
            jump_table_density_threshold: None,
        };

        ZkSolcSettings {
            libraries,
            optimizer,
            evm_version: Some(evm_version),
            metadata: Some(SettingsMetadata { bytecode_hash: self.bytecode_hash }),
            via_ir: Some(via_ir),
            // Set in project paths.
            remappings: Vec::new(),
            detect_missing_libraries: self.detect_missing_libraries,
            enable_eravm_extensions: self.enable_eravm_extensions,
            force_evmla: self.force_evmla,
            llvm_options: self.llvm_options.clone(),
            output_selection: OutputSelection {
                all: Some(FileOutputSelection {
                    per_file: None,
                    per_contract: Some([OutputSelectionFlag::ABI].into()),
                }),
            },
            solc: self.solc_path.clone(),
        }
    }

    pub fn avoid_contracts(&self) -> Option<Vec<globset::GlobMatcher>> {
        self.avoid_contracts.clone().map(|patterns| {
            patterns
                .into_iter()
                .map(|pat| globset::Glob::new(&pat).expect("invalid pattern").compile_matcher())
                .collect::<Vec<_>>()
        })
    }
}
