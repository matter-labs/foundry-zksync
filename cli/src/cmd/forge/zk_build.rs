/// The `zkbuild` module provides comprehensive functionality for the compilation of zkSync
/// smart contracts with a specified Solidity compiler version.
///
/// This module consists of the following key structures and their corresponding
/// implementations:
///
/// * `ZkBuildArgs`: This structure encapsulates the parameters necessary for building zkSync
///   contracts. These parameters include the Solidity compiler version, an option to enable
///   the system contract compilation mode, and an option to forcibly switch to the EVM legacy
///   assembly pipeline. Additionally, it includes core build arguments defined in the
///   `CoreBuildArgs` structure.
///
/// * `Cmd` Implementation for `ZkBuildArgs`: This implementation includes a `run` function,
///   which initiates the zkSync contract compilation process. The `run` function takes care of
///   the various steps involved in the process, including downloading the zksolc compiler,
///   setting up the compiler directory, and invoking the compilation process.
///
/// This module serves as a facilitator for the zkSync contract compilation process. It allows
/// developers to specify the compiler version and compilation options and handles the
/// intricate tasks necessary for contract compilation. This includes downloading the specified
/// compiler version if it's not already available, preparing the compiler's directory, and
/// finally, invoking the compilation process with the provided options.
///
/// The `zkbuild` module is part of a larger framework aimed at managing and interacting with
/// zkSync contracts. It is designed to provide a seamless experience for developers, providing
/// an easy-to-use interface for contract compilation while taking care of the underlying
/// complexities.
use super::build::CoreBuildArgs;
use super::{
    zksolc::{ZkSolc, ZkSolcOpts},
    zksolc_manager::{
        ZkSolcManager, ZkSolcManagerBuilder, ZkSolcManagerOpts, DEFAULT_ZKSOLC_VERSION,
    },
};
use crate::cmd::{Cmd, LoadConfig};
use clap::Parser;
use ethers::prelude::Project;
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
use std::fmt::Debug;

foundry_config::merge_impl_figment_convert!(ZkBuildArgs, args);

/// The `ZkBuildArgs` struct encapsulates the parameters required for the zkSync contract
/// compilation process.
///
/// This includes:
/// * `use_zksolc`: The version of the Solidity compiler (solc) to be used for compilation, or the
///   path to a local solc. The values can be in the format `x.y.z`, `solc:x.y.z`, or
///   `path/to/solc`. It is used to specify the compiler version or location, which is crucial for
///   the contract building process.
///
/// * `is_system`: A boolean flag indicating whether to enable the system contract compilation mode.
///   In this mode, zkEVM extensions are enabled, for example, calls to addresses `0xFFFF` and below
///   are substituted by special zkEVM instructions. This option is used when we want to compile
///   system contracts.
///
/// * `force_evmla`: A boolean flag indicating whether to forcibly switch to the EVM legacy assembly
///   pipeline. This is useful for older revisions of `solc` 0.8, where Yul was considered highly
///   experimental and contained more bugs than today. This flag allows us to use the EVM legacy
///   assembly pipeline, which can be beneficial in certain situations.
///
/// * `args`: Core build arguments encapsulated in the `CoreBuildArgs` struct. These include
///   additional parameters required for building the contract, such as optimization level, output
///   directory etc.
///
/// This struct is used as input to the `ZkSolc` compiler, which will use these arguments to
/// configure the compilation process. It implements the `Cmd` trait, which triggers the compilation
/// process when the `run` function is called. The struct also implements the `Provider` trait,
/// allowing it to be converted into a form that can be merged into the application's configuration
/// object.
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
        default_value = DEFAULT_ZKSOLC_VERSION
    )]
    #[serde(skip)]
    pub use_zksolc: String,

    /// A flag indicating whether to enable the system contract compilation mode.
    #[clap(
        help_heading = "ZkSync Compiler options",
        help = "Enable the system contract compilation mode. In this mode zkEVM extensions are enabled. For example, calls
        to addresses `0xFFFF` and below are substituted by special zkEVM instructions.",
        long = "is-system",
        value_name = "SYSTEM_MODE"
    )]
    pub is_system: bool,

    /// A flag indicating whether to forcibly switch to the EVM legacy assembly pipeline.
    #[clap(
        help_heading = "ZkSync Compiler options",
        help = "Forcibly switch to the EVM legacy assembly pipeline. It is useful for older revisions of `solc` 0.8, where
        Yul was considered highly experimental and contained more bugs than today",
        long = "force-evmla",
        value_name = "FORCE_EVMLA"
    )]
    pub force_evmla: bool,

    /// Core build arguments encapsulated in the `CoreBuildArgs` struct.
    #[clap(flatten)]
    #[serde(flatten)]
    pub args: CoreBuildArgs,
}

impl Cmd for ZkBuildArgs {
    type Output = ();

    /// Executes the zkSync contract compilation process based on the parameters encapsulated in the
    /// `ZkBuildArgs` instance.
    ///
    /// This method performs the following steps:
    /// 1. Tries to load the application's configuration, emitting warnings if any issues are
    ///    encountered.
    /// 2. Modifies the project's artifact path to be the "zkout" directory in the project's root
    ///    directory.
    /// 3. Creates a `ZkSolcManager` instance based on the specified zkSync Solidity compiler
    ///    (`use_zksolc` field in `ZkBuildArgs`).
    /// 4. Checks if the setup compilers directory is properly set up. If not, it raises an error
    ///    and halts execution.
    /// 5. If the zkSync Solidity compiler does not exist in the compilers directory, it triggers
    ///    its download.
    /// 6. Initiates the contract compilation process using the `ZkSolc` compiler. This process is
    ///    configured with the `is_system` and `force_evmla` parameters from the `ZkBuildArgs`
    ///    instance, and the path to the zkSync Solidity compiler.
    /// 7. If the compilation process fails, it raises an error and halts execution.
    ///
    /// The method returns `Ok(())` if the entire process completes successfully, or an error if any
    /// step in the process fails. The purpose of this function is to consolidate all steps
    /// involved in the zkSync contract compilation process in a single method, allowing for
    /// easy invocation of the process with a single function call.
    fn run(self) -> eyre::Result<()> {
        let config = self.try_load_config_emit_warnings()?;
        let mut project = config.project()?;

        //set zk out path
        let zk_out_path = project.paths.root.join("zkout");
        project.paths.artifacts = zk_out_path;

        let zksolc_manager = self.setup_zksolc_manager()?;

        println!("Compiling smart contracts...");
        self.compile_smart_contracts(zksolc_manager, project)
    }
}

impl ZkBuildArgs {
    /// The `setup_zksolc_manager` function creates and prepares an instance of `ZkSolcManager`.
    ///
    /// It follows these steps:
    /// 1. Instantiate `ZkSolcManagerOpts` and `ZkSolcManagerBuilder` with the specified zkSync
    ///    Solidity compiler.
    /// 2. Create a `ZkSolcManager` using the builder.
    /// 3. Check if the setup compilers directory is properly set up. If not, it raises an error.
    /// 4. If the zkSync Solidity compiler does not exist in the compilers directory, it triggers
    ///    its download.
    ///
    /// The function returns the `ZkSolcManager` if all steps are successful, or an error if any
    /// step fails.
    fn setup_zksolc_manager(&self) -> eyre::Result<ZkSolcManager> {
        let zksolc_manager_opts = ZkSolcManagerOpts::new(self.use_zksolc.clone());
        let zksolc_manager_builder = ZkSolcManagerBuilder::new(zksolc_manager_opts);
        let zksolc_manager = zksolc_manager_builder
            .build()
            .map_err(|e| eyre::eyre!("Error building zksolc_manager: {}", e))?;

        if let Err(err) = zksolc_manager.check_setup_compilers_dir() {
            eyre::bail!("Failed to setup compilers directory: {}", err);
        }

        if !zksolc_manager.exists() {
            println!(
                "Downloading zksolc compiler from {:?}",
                zksolc_manager.get_full_download_url().unwrap().to_string()
            );
            zksolc_manager
                .download()
                .map_err(|err| eyre::eyre!("Failed to download the file: {}", err))?;
        }

        Ok(zksolc_manager)
    }

    /// The `compile_smart_contracts` function initiates the contract compilation process.
    ///
    /// It follows these steps:
    /// 1. Create an instance of `ZkSolcOpts` with the appropriate options.
    /// 2. Instantiate `ZkSolc` with the created options and the project.
    /// 3. Initiate the contract compilation process.
    ///
    /// The function returns `Ok(())` if the compilation process completes successfully, or an error
    /// if it fails.
    fn compile_smart_contracts(
        &self,
        zksolc_manager: ZkSolcManager,
        project: Project,
    ) -> eyre::Result<()> {
        let zksolc_opts = ZkSolcOpts {
            compiler_path: zksolc_manager.get_full_compiler_path(),
            is_system: self.is_system,
            force_evmla: self.force_evmla,
        };

        let zksolc = ZkSolc::new(zksolc_opts, project);

        match zksolc.compile() {
            Ok(_) => {
                println!("Compiled Successfully");
                Ok(())
            }
            Err(err) => {
                eyre::bail!("Failed to compile smart contracts with zksolc: {}", err);
            }
        }
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
