use foundry_compilers::{
    artifacts::Libraries,
    zksync::artifacts::{BytecodeHash, Optimizer, OptimizerDetails, Settings, SettingsMetadata},
    EvmVersion,
};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::SolcReq;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZkOptimizerConfig {
    /// Optimizer settings for zkSync
    pub enable: bool,

    /// The optimization mode string.
    pub mode: char,

    /// zksolc optimizer details remain the same
    pub details: Option<OptimizerDetails>,
}

impl Default for ZkOptimizerConfig {
    fn default() -> Self {
        Self { enable: Default::default(), mode: '3', details: Default::default() }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZkCompilerConfig {
    /// The zkSolc instance to use if any.
    pub zksolc: Option<SolcReq>,

    /// solc path to use along the zksolc compiler
    pub solc: Option<PathBuf>,

    /// Whether to include the metadata hash for zksolc compiled bytecode.
    pub bytecode_hash: BytecodeHash,

    /// Optimizer configuration
    pub optimizer: ZkOptimizerConfig,

    /// Whether to try to recompile with -Oz if the bytecode is too large.
    pub fallback_oz: bool,

    /// Whether to support compilation of zkSync-specific simulations
    pub enable_eravm_extensions: bool,

    /// Force evmla for zkSync
    pub force_evmla: bool,

    /// Path to cache missing library dependencies, used for compiling and deploying libraries.
    pub detect_missing_libraries: bool,

    /// Source files to avoid compiling on zksolc
    pub avoid_contracts: Option<Vec<String>>,
}

impl ZkCompilerConfig {
    pub fn settings(
        &self,
        libraries: Libraries,
        evm_version: EvmVersion,
        via_ir: bool,
    ) -> Settings {
        let optimizer = Optimizer {
            enabled: Some(self.optimizer.enable),
            mode: Some(self.optimizer.mode),
            fallback_to_optimizing_for_size: Some(self.fallback_oz),
            disable_system_request_memoization: Some(true),
            details: self.optimizer.details.clone(),
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
            system_mode: self.enable_eravm_extensions,
            force_evmla: self.force_evmla,
            // TODO: See if we need to set this from here
            output_selection: Default::default(),
            solc: self.solc.clone(),
        }
    }
}

#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZkSyncConfig {
    /// Enable zksync mode
    pub enable: bool,

    /// zkSync compiler
    pub compiler: ZkCompilerConfig,
}

impl ZkSyncConfig {
    /// Returns true if zk mode is enabled and it if tests should be run in zk mode
    pub fn run_in_zk_mode(&self) -> bool {
        self.enable
    }
}
