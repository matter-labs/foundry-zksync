use std::path::PathBuf;

use clap::{builder::PossibleValue, Parser, ValueEnum};
use foundry_config::ZkSyncConfig;

/// Represents the zkVM mode setting
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
enum Mode {
    /// Run in zkVM mode
    ///
    /// This will run tests & scripts with zkVM mode enabled at startup
    #[default]
    Run,
    /// Compile contracts for zkSync
    ///
    /// This allows tests & scripts to be run in EVM mode and switch to zkVM mode during execution
    Compile,
}

impl ValueEnum for Mode {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Run, Self::Compile]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(match self {
            Mode::Run => PossibleValue::new("run").alias("r"),
            Mode::Compile => PossibleValue::new("compile").alias("compile-only").alias("c"),
        })
    }
}

#[derive(Clone, Debug, Default, Parser)]
#[clap(next_help_heading = "ZKSync configuration")]
pub struct ZkSyncArgs {
    /// Use ZKSync era vm.
    #[clap(long = "zksync", num_args = 0..=1, require_equals = true, default_missing_value = "run")]
    mode: Option<Mode>,

    #[clap(
        help = "Solc compiler path to use when compiling with zksolc",
        long = "zk-solc-path",
        value_name = "ZK_SOLC_PATH"
    )]
    pub solc_path: Option<PathBuf>,

    /// A flag indicating whether to enable the system contract compilation mode.
    #[clap(
        help = "Enable the system contract compilation mode.",
        long = "zk-eravm-extensions",
        visible_alias = "enable-eravm-extensions",
        visible_alias = "system-mode",
        value_name = "ENABLE_ERAVM_EXTENSIONS",
        default_missing_value = "true"
    )]
    pub eravm_extensions: Option<bool>,

    /// A flag indicating whether to forcibly switch to the EVM legacy assembly pipeline.
    #[clap(
        help = "Forcibly switch to the EVM legacy assembly pipeline.",
        long = "zk-force-evmla",
        visible_alias = "force-evmla",
        value_name = "FORCE_EVMLA",
        default_missing_value = "true"
    )]
    pub force_evmla: Option<bool>,

    /// Try to recompile with -Oz if the bytecode is too large.
    #[clap(
        long = "zk-fallback-oz",
        visible_alias = "fallback-oz",
        value_name = "FALLBACK_OZ",
        default_missing_value = "true"
    )]
    pub fallback_oz: Option<bool>,

    /// Detect missing libraries, instead of erroring
    ///
    /// Currently unused
    #[clap(long = "zk-detect-missing-libraries", default_missing_value = "true")]
    pub detect_missing_libraries: bool,

    /// Set the LLVM optimization parameter `-O[0 | 1 | 2 | 3 | s | z]`.
    /// Use `3` for best performance and `z` for minimal size.
    #[clap(
        short = 'O',
        long = "zk-optimizer-mode",
        visible_alias = "zk-optimization",
        value_name = "LEVEL"
    )]
    pub optimizer_mode: Option<String>,

    /// Enables optimizations
    #[clap(long = "zk-optimizer", default_missing_value = "true")]
    pub optimizer: bool,

    /// Contracts to avoid compiling on zkSync
    #[clap(long = "zk-avoid-contracts", visible_alias = "avoid-contracts", value_delimiter = ',')]
    pub avoid_contracts: Option<Vec<String>>,
}

impl ZkSyncArgs {
    /// Returns true if zksync mode is enabled
    pub fn enabled(&self) -> bool {
        self.mode.is_some()
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

        set_if_some!(self.mode.map(|_| true), zksync.enable);
        set_if_some!(self.mode.map(|zkvm| zkvm == Mode::Compile), zksync.compile_only);

        set_if_some!(self.solc_path.clone(), zksync.solc_path);
        set_if_some!(self.eravm_extensions, zksync.eravm_extensions);
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
