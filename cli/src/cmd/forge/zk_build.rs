/// The `zkbuild` module provides functionalities to build zkSync contracts with a specified Solidity compiler version.
///
/// This module consists of several important structures and their corresponding implementations:
///
/// * `ZkBuildArgs`: This structure encapsulates the parameters required for building zkSync contracts, including the
///   Solidity compiler version, whether to enable the system contract compilation mode, and whether to forcibly switch
///   to the EVM legacy assembly pipeline. It also includes core build arguments defined in the `CoreBuildArgs` structure.
///
/// * This implementation includes a `run` function, which triggers the
///   compilation process of zkSync contracts. It handles the downloading of the zksolc compiler,
///   setting up the compiler directory, and invoking the compilation process.
///
/// This module facilitates the zkSync contract compilation process. It provides a way for
/// developers to specify the compiler version and compilation options, and it handles the underlying tasks necessary
/// to compile the contracts. This includes downloading the specified compiler version if it's not already present,
/// preparing the compiler's directory, and finally invoking the compilation process with the specified options.
///
/// This module is part of a larger framework for managing and interacting with zkSync contracts.
use std::fmt::Debug;

use super::build::CoreBuildArgs;
use super::zksolc::{ZkSolc, ZkSolcOpts};
use super::zksolc_manager::{ZkSolcManagerBuilder, ZkSolcManagerOpts};
use crate::cmd::{Cmd, LoadConfig};
use clap::Parser;
use foundry_config::{
    figment::{
        self,
        error::Kind::InvalidType,
        value::{Dict, Map, Value},
        Metadata, Profile, Provider,
    },
    Config,
};
use serde::Serialize;

foundry_config::merge_impl_figment_convert!(ZkBuildArgs, args);

/// The `ZkBuildArgs` struct encapsulates the parameters required for the zkSync contract compilation process.
///
/// This includes:
/// * `use_zksolc`: The version of the Solidity compiler (solc) to be used for compilation, or the path to a local solc.
///   The values can be in the format `x.y.z`, `solc:x.y.z`, or `path/to/solc`.
/// * `is_system`: A boolean flag indicating whether to enable the system contract compilation mode. In this mode,
///   zkEVM extensions are enabled, for example, calls to addresses `0xFFFF` and below are substituted by special zkEVM instructions.
/// * `force_evmla`: A boolean flag indicating whether to forcibly switch to the EVM legacy assembly pipeline. This is
///   useful for older revisions of `solc` 0.8, where Yul was considered highly experimental and contained more bugs than today.
/// * `args`: Core build arguments encapsulated in the `CoreBuildArgs` struct.
///
/// This struct is used as input to the `ZkSolc` compiler, which will use these arguments to configure the compilation process.
/// It implements the `Cmd` trait, which triggers the compilation process when the `run` function is called. The struct also
/// implements the `Provider` trait, allowing it to be converted into a form that can be merged into the application's configuration object.
#[derive(Debug, Clone, Parser, Serialize, Default)]
#[clap(next_help_heading = "ZkBuild options", about = None)]
pub struct ZkBuildArgs {
    /// Specify the solc version, or a path to a local solc, to build with.
    ///
    /// Valid values are in the format `x.y.z`, `solc:x.y.z` or `path/to/solc`.
    #[clap(
        help_heading = "ZkSync Compiler options",
        value_name = "ZK_SOLC_VERSION",
        long = "use-zksolc",
        default_value = "v1.3.9"
    )]
    #[serde(skip)]
    pub use_zksolc: String,

    #[clap(
        help_heading = "ZkSync Compiler options",
        help = "Enable the system contract compilation mode. In this mode zkEVM extensions are enabled. For example, calls
        to addresses `0xFFFF` and below are substituted by special zkEVM instructions.",
        long = "is-system",
        value_name = "SYSTEM_MODE"
    )]
    pub is_system: bool,

    #[clap(
        help_heading = "ZkSync Compiler options",
        help = "Forcibly switch to the EVM legacy assembly pipeline. It is useful for older revisions of `solc` 0.8, where
        Yul was considered highly experimental and contained more bugs than today",
        long = "force-evmla",
        value_name = "FORCE_EVMLA"
    )]
    pub force_evmla: bool,

    #[clap(flatten)]
    #[serde(flatten)]
    pub args: CoreBuildArgs,
}

impl Cmd for ZkBuildArgs {
    type Output = ();

    /// Executes the zkSync contract compilation process based on the parameters encapsulated in the `ZkBuildArgs` instance.
    ///
    /// This method performs the following steps:
    /// 1. Tries to load the application's configuration, emitting warnings if any issues are encountered.
    /// 2. Modifies the project's artifact path to be the "zkout" directory in the project's root directory.
    /// 3. Creates a `ZkSolcManager` instance based on the specified zkSync Solidity compiler (`use_zksolc` field in `ZkBuildArgs`).
    /// 4. Checks if the setup compilers directory is properly set up. If not, it raises an error and halts execution.
    /// 5. If the zkSync Solidity compiler does not exist in the compilers directory, it triggers its download.
    /// 6. Initiates the contract compilation process using the `ZkSolc` compiler. This process is configured with the
    ///    `is_system` and `force_evmla` parameters from the `ZkBuildArgs` instance, and the path to the zkSync Solidity compiler.
    /// 7. If the compilation process fails, it raises an error and halts execution.
    ///
    /// The method returns `Ok(())` if the entire process completes successfully, or an error if any step in the process fails.
    /// The purpose of this function is to consolidate all steps involved in the zkSync contract compilation process in a single method,
    /// allowing for easy invocation of the process with a single function call.

    fn run(self) -> eyre::Result<()> {
        let config = self.try_load_config_emit_warnings()?;
        let mut project = config.project()?;

        //set zk out path
        let zk_out_path = project.paths.root.join("zkout");
        project.paths.artifacts = zk_out_path;

        let zksolc_manager_opts = ZkSolcManagerOpts::new(self.use_zksolc);
        let zksolc_manager_builder = ZkSolcManagerBuilder::new(zksolc_manager_opts);
        let zksolc_manager = &zksolc_manager_builder.build();

        match zksolc_manager {
            Ok(zksolc_manager) => {
                if let Err(err) = zksolc_manager.check_setup_compilers_dir() {
                    eyre::bail!("Failed to setup compilers directory: {}", err);
                }

                if !zksolc_manager.exists() {
                    println!("Downloading zksolc compiler");

                    match zksolc_manager.download() {
                        Ok(zksolc_manager) => zksolc_manager,
                        Err(err) => {
                            eyre::bail!("Failed to download the file: {}", err);
                        }
                    }
                }

                println!("Compiling smart contracts...");

                let zksolc_opts = ZkSolcOpts {
                    compiler_path: zksolc_manager.get_full_compiler_path(),
                    //we may not add these yet as they may be file specific
                    is_system: self.is_system,
                    force_evmla: self.force_evmla,
                };

                let zksolc = ZkSolc::new(zksolc_opts, project);

                match zksolc.compile() {
                    Ok(_) => println!("Compiled Successfully"),
                    Err(err) => {
                        eyre::bail!("Failed to compile smart contracts with zksolc: {}", err);
                    }
                }
            }
            Err(e) => eyre::bail!("Error building zksolc_manager: {}", e),
        }

        Ok(())
    }
}

// Make this args a `figment::Provider` so that it can be merged into the `Config`
impl Provider for ZkBuildArgs {
    fn metadata(&self) -> Metadata {
        Metadata::named("Build Args Provider")
    }

    fn data(&self) -> Result<Map<Profile, Dict>, figment::Error> {
        let value = Value::serialize(self)?;
        let error = InvalidType(value.to_actual(), "map".into());
        let dict = value.into_dict().ok_or(error)?;

        // if self.names {
        //     dict.insert("names".to_string(), true.into());
        // }

        // if self.sizes {
        //     dict.insert("sizes".to_string(), true.into());
        // }

        Ok(Map::from([(Config::selected_profile(), dict)]))
    }
}
