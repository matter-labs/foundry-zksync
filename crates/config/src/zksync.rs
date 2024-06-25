use foundry_compilers::{
    artifacts::Libraries,
    zksync::artifacts::{BytecodeHash, Optimizer, OptimizerDetails, Settings, SettingsMetadata},
    EvmVersion,
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
    pub bytecode_hash: BytecodeHash,

    /// Whether to try to recompile with -Oz if the bytecode is too large.
    pub fallback_oz: bool,

    /// Whether to support compilation of zkSync-specific simulations
    pub eravm_extensions: bool,

    /// Force evmla for zkSync
    pub force_evmla: bool,

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
            eravm_extensions: Default::default(),
            force_evmla: Default::default(),
            detect_missing_libraries: Default::default(),
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
    ) -> Settings {
        let optimizer = Optimizer {
            enabled: Some(self.optimizer),
            mode: Some(self.optimizer_mode),
            fallback_to_optimizing_for_size: Some(self.fallback_oz),
            disable_system_request_memoization: Some(true),
            details: self.optimizer_details.clone(),
            jump_table_density_threshold: None,
        };

        Settings {
            libraries,
            optimizer,
            evm_version: Some(evm_version),
            metadata: Some(SettingsMetadata { bytecode_hash: Some(self.bytecode_hash) }),
            via_ir: Some(via_ir),
            // Set in project paths.
            remappings: Vec::new(),
            detect_missing_libraries: self.detect_missing_libraries,
            system_mode: self.eravm_extensions,
            force_evmla: self.force_evmla,
            // TODO: See if we need to set this from here
            output_selection: Default::default(),
            solc: self.solc_path.clone(),
        }
    }
}
