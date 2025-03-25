use std::{collections::HashSet, path::PathBuf};

use alloy_primitives::{hex, Address, Bytes};
use clap::Parser;
use foundry_config::zksync::ZkSyncConfig;
use foundry_zksync_compilers::compilers::zksolc::{ErrorType, WarningType};
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

    /// Paymaster address
    #[clap(
        long = "zk-paymaster-address",
        value_name = "PAYMASTER_ADDRESS",
        visible_alias = "paymaster-address"
    )]
    pub paymaster_address: Option<Address>,

    /// Paymaster input
    #[clap(
        long = "zk-paymaster-input",
        value_name = "PAYMASTER_INPUT",
        visible_alias = "paymaster-input",
        value_parser = parse_hex_bytes
    )]
    pub paymaster_input: Option<Bytes>,

    /// Set the warnings to suppress for zksolc.
    #[clap(
        long = "zk-suppressed-warnings",
        alias = "suppressed-warnings",
        visible_alias = "suppress-warnings",
        value_delimiter = ',',
        help = "Set the warnings to suppress for zksolc, possible values: [txorigin, assemblycreate]"
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppressed_warnings: Option<Vec<WarningType>>,

    /// Set the errors to suppress for zksolc.
    #[clap(
        long = "zk-suppressed-errors",
        alias = "suppressed-errors",
        visible_alias = "suppress-errors",
        value_delimiter = ',',
        help = "Set the errors to suppress for zksolc, possible values: [sendtransfer]"
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppressed_errors: Option<Vec<ErrorType>>,
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

        set_if_some!(self.optimizer.then_some(true), zksync.optimizer);
        set_if_some!(
            self.optimizer_mode.as_ref().and_then(|mode| mode.parse::<char>().ok()),
            zksync.optimizer_mode
        );
        let suppressed_warnings = self
            .suppressed_warnings
            .clone()
            .map(|values| values.into_iter().collect::<HashSet<_>>());
        set_if_some!(suppressed_warnings, zksync.suppressed_warnings);
        let suppressed_errors =
            self.suppressed_errors.clone().map(|values| values.into_iter().collect::<HashSet<_>>());
        set_if_some!(suppressed_errors, zksync.suppressed_errors);

        zksync
    }
}

fn parse_hex_bytes(s: &str) -> Result<Bytes, String> {
    hex::decode(s).map(Bytes::from).map_err(|e| format!("Invalid hex string: {e}"))
}
