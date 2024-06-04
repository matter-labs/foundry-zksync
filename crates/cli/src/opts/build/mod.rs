use std::path::PathBuf;

use clap::Parser;
use foundry_compilers::{artifacts::output_selection::ContractOutputSelection, EvmVersion};
use serde::Serialize;

mod core;
pub use self::core::CoreBuildArgs;

mod paths;
pub use self::paths::ProjectPathsArgs;

// A set of solc compiler settings that can be set via command line arguments, which are intended
// to be merged into an existing `foundry_config::Config`.
//
// See also `BuildArgs`.
#[derive(Clone, Debug, Default, Serialize, Parser)]
#[clap(next_help_heading = "Compiler options")]
pub struct CompilerArgs {
    /// Includes the AST as JSON in the compiler output.
    #[clap(long, help_heading = "Compiler options")]
    #[serde(skip)]
    pub ast: bool,

    /// The target EVM version.
    #[clap(long, value_name = "VERSION")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evm_version: Option<EvmVersion>,

    /// Activate the Solidity optimizer.
    #[clap(long)]
    #[serde(skip)]
    pub optimize: bool,

    /// The number of optimizer runs.
    #[clap(long, value_name = "RUNS")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimizer_runs: Option<usize>,

    /// Extra output to include in the contract's artifact.
    ///
    /// Example keys: evm.assembly, ewasm, ir, irOptimized, metadata
    ///
    /// For a full description, see https://docs.soliditylang.org/en/v0.8.13/using-the-compiler.html#input-description
    #[clap(long, num_args(1..), value_name = "SELECTOR")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extra_output: Vec<ContractOutputSelection>,

    /// Extra output to write to separate files.
    ///
    /// Valid values: metadata, ir, irOptimized, ewasm, evm.assembly
    #[clap(long, num_args(1..), value_name = "SELECTOR")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extra_output_files: Vec<ContractOutputSelection>,

    // @zksync
    /// Use ZKSync era vm.
    #[clap(help_heading = "Use ZKSync era vm", long)]
    pub zksync: bool,

    #[clap(
        help_heading = "zkSync Compiler options",
        help = "Solc compiler path to use when compiling with zksolc",
        long = "zk-solc-path",
        value_name = "ZK_SOLC_PATH"
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zk_solc_path: Option<PathBuf>,

    /// A flag indicating whether to enable the system contract compilation mode.
    #[clap(
        help_heading = "zkSync Compiler options",
        help = "Enable the system contract compilation mode.",
        long = "is-system",
        value_name = "SYSTEM_MODE"
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_system: Option<bool>,

    /// A flag indicating whether to forcibly switch to the EVM legacy assembly pipeline.
    #[clap(
        help_heading = "zkSync Compiler options",
        help = "Forcibly switch to the EVM legacy assembly pipeline.",
        long = "force-evmla",
        value_name = "FORCE_EVMLA"
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_evmla: Option<bool>,

    /// Try to recompile with -Oz if the bytecode is too large.
    #[clap(
        help_heading = "zkSync Compiler options",
        long = "fallback-oz",
        value_name = "FALLBACK_OZ"
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_oz: Option<bool>,

    /// Path to cache missing library dependencies, used for compiling and deploying libraries.
    #[clap(help_heading = "zkSync Compiler options", long = "detect-missing-libraries")]
    pub detect_missing_libraries: bool,

    /// Set the LLVM optimization parameter `-O[0 | 1 | 2 | 3 | s | z]`.
    /// Use `3` for best performance and `z` for minimal size.
    #[clap(
        help_heading = "zkSync Compiler options",
        short = 'O',
        long = "optimization",
        value_name = "LEVEL"
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    /// Enables optimizations
    #[clap(help_heading = "zkSync Compiler options", long = "zk-optimizer")]
    #[serde(skip)]
    pub zk_optimizer: bool,

    /// Contracts to avoid compiling on zkSync
    #[clap(
        long,
        help_heading = "Contracts to avoid during zkSync compilation",
        value_delimiter = ','
    )]
    pub avoid_contracts: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_parse_evm_version() {
        let args: CompilerArgs =
            CompilerArgs::parse_from(["foundry-cli", "--evm-version", "london"]);
        assert_eq!(args.evm_version, Some(EvmVersion::London));
    }

    #[test]
    fn can_parse_extra_output() {
        let args: CompilerArgs =
            CompilerArgs::parse_from(["foundry-cli", "--extra-output", "metadata", "ir-optimized"]);
        assert_eq!(
            args.extra_output,
            vec![ContractOutputSelection::Metadata, ContractOutputSelection::IrOptimized]
        );
    }

    #[test]
    fn can_parse_extra_output_files() {
        let args: CompilerArgs = CompilerArgs::parse_from([
            "foundry-cli",
            "--extra-output-files",
            "metadata",
            "ir-optimized",
        ]);
        assert_eq!(
            args.extra_output_files,
            vec![ContractOutputSelection::Metadata, ContractOutputSelection::IrOptimized]
        );
    }
}
