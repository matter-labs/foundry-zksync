use std::path::PathBuf;

use clap::Parser;
use foundry_config::ZkSyncConfig;
use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize, Parser)]
#[clap(next_help_heading = "ZKSync configuration")]
pub struct ZkSyncArgs {
    /// Compile for zkVM
    #[clap(
        long = "zk-compile",
        value_name = "COMPILE_FOR_ZKVM",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        default_value_if("startup", "true", "true"))]
    pub compile: Option<bool>,

    /// Enable zkVM at startup
    #[clap(
        long = "zk-startup",
        visible_alias = "zksync",
        display_order = 0,
        value_name = "ENABLE_ZKVM_AT_STARTUP",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
    )]
    pub startup: Option<bool>,

    #[clap(
        help = "Solc compiler path to use when compiling with zksolc",
        long = "zk-solc-path",
        value_name = "ZK_SOLC_PATH"
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solc_path: Option<PathBuf>,

    /// A flag indicating whether to enable the system contract compilation mode.
    #[clap(
        help = "Enable the system contract compilation mode.",
        long = "zk-enable-eravm-extensions",
        visible_alias = "enable-eravm-extensions",
        visible_alias = "system-mode",
        value_name = "ENABLE_ERAVM_EXTENSIONS",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true"
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_eravm_extensions: Option<bool>,

    /// A flag indicating whether to forcibly switch to the EVM legacy assembly pipeline.
    #[clap(
        help = "Forcibly switch to the EVM legacy assembly pipeline.",
        long = "zk-force-evmla",
        visible_alias = "force-evmla",
        value_name = "FORCE_EVMLA",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true"
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_evmla: Option<bool>,

    /// ZkSolc extra LLVM options
    #[clap(help = "ZkSolc extra LLVM options", long = "zk-llvm-options")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llvm_options: Option<Vec<String>>,

    /// Try to recompile with -Oz if the bytecode is too large.
    #[clap(
        long = "zk-fallback-oz",
        visible_alias = "fallback-oz",
        value_name = "FALLBACK_OZ",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true"
    )]
    pub fallback_oz: Option<bool>,

    /// Detect missing libraries, instead of erroring
    ///
    /// Currently unused
    #[clap(long = "zk-detect-missing-libraries")]
    pub detect_missing_libraries: bool,

    /// Set the LLVM optimization parameter `-O[0 | 1 | 2 | 3 | s | z]`.
    /// Use `3` for best performance and `z` for minimal size.
    #[clap(
        short = 'O',
        long = "zk-optimizer-mode",
        visible_alias = "zk-optimization",
        value_name = "LEVEL"
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimizer_mode: Option<String>,

    /// Enables optimizations
    #[clap(long = "zk-optimizer")]
    pub optimizer: bool,

    /// Contracts to avoid compiling on zkSync
    #[clap(long = "zk-avoid-contracts", visible_alias = "avoid-contracts", value_delimiter = ',')]
    pub avoid_contracts: Option<Vec<String>>,
}

impl ZkSyncArgs {
    /// Returns true if zksync mode is enabled
    pub fn enabled(&self) -> bool {
        self.compile.unwrap_or_default()
    }

    /// Merge the current cli arguments into the specified zksync configuration
    pub(crate) fn apply_overrides(&self, mut zksync: ZkSyncConfig) -> ZkSyncConfig {
        macro_rules! set_if_some {
            ($src:expr, $dst:expr) => {
                if let Some(src) = $src {
                    $dst = src.into();
                }
            };
        }

        set_if_some!(self.compile, zksync.compile);
        set_if_some!(self.startup, zksync.startup);
        set_if_some!(self.solc_path.clone(), zksync.solc_path);
        set_if_some!(self.enable_eravm_extensions, zksync.enable_eravm_extensions);
        set_if_some!(self.llvm_options.clone(), zksync.llvm_options);
        set_if_some!(self.force_evmla, zksync.force_evmla);
        set_if_some!(self.fallback_oz, zksync.fallback_oz);
        set_if_some!(
            self.detect_missing_libraries.then_some(true),
            zksync.detect_missing_libraries
        );
        set_if_some!(self.avoid_contracts.clone(), zksync.avoid_contracts);

        set_if_some!(self.optimizer.then_some(true), zksync.optimizer);
        set_if_some!(
            self.optimizer_mode.as_ref().and_then(|mode| mode.parse::<char>().ok()),
            zksync.optimizer_mode
        );

        zksync
    }
}
