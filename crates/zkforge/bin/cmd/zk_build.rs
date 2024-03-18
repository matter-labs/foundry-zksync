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
use super::{install, watch::WatchArgs};
use clap::Parser;
use foundry_cli::{opts::CoreBuildArgs, utils::LoadConfig};
use foundry_common::zksolc_manager::setup_zksolc_manager;
use foundry_compilers::Project;
use foundry_config::{
    figment::{
        self,
        error::Kind::InvalidType,
        value::{Dict, Map, Value},
        Metadata, Profile, Provider,
    },
    Config,
};
use itertools::Itertools;
use serde::Serialize;
use std::fmt::Debug;
use watchexec::config::{InitConfig, RuntimeConfig};

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
    /// Print compiled contract names.
    #[clap(long)]
    #[serde(skip)]
    pub names: bool,

    /// Print compiled contract sizes.
    #[clap(long)]
    #[serde(skip)]
    pub sizes: bool,

    /// Core build arguments encapsulated in the `CoreBuildArgs` struct.
    #[clap(flatten)]
    #[serde(flatten)]
    pub args: CoreBuildArgs,

    #[clap(flatten)]
    #[serde(skip)]
    pub watch: WatchArgs,
}

impl ZkBuildArgs {
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
    pub async fn run(self) -> eyre::Result<()> {
        let mut config = self.try_load_config_emit_warnings()?;
        let mut project = config.project()?;
        let mut zksolc_cfg = config.zk_solc_config().map_err(|e| eyre::eyre!(e))?;
        zksolc_cfg.contracts_to_compile =
            self.args.compiler.contracts_to_compile.clone().map(|patterns| {
                patterns
                    .into_iter()
                    .map(|pat| globset::Glob::new(&pat).unwrap().compile_matcher())
                    .collect_vec()
            });
        zksolc_cfg.avoid_contracts = self.args.compiler.avoid_contracts.clone().map(|patterns| {
            patterns
                .into_iter()
                .map(|pat| globset::Glob::new(&pat).unwrap().compile_matcher())
                .collect_vec()
        });
        //set zk out path
        let zk_out_path = project.paths.root.join("zkout");
        project.paths.artifacts = zk_out_path;

        if install::install_missing_dependencies(&mut config, self.args.silent) &&
            config.auto_detect_remappings
        {
            // need to re-configure here to also catch additional remappings
            config = self.load_config();
            project = config.project()?;
            zksolc_cfg = config.zk_solc_config().map_err(|e| eyre::eyre!(e))?;
        }

        // TODO: revisit to remove zksolc_manager and move binary installation to zksolc_config
        let compiler_path = setup_zksolc_manager(self.args.use_zksolc.clone()).await?;
        zksolc_cfg.compiler_path = compiler_path;

        // TODO: add filter support
        foundry_common::zk_compile::compile_smart_contracts(zksolc_cfg, project)?;

        Ok(())
    }
    /// Returns the `Project` for the current workspace
    ///
    /// This loads the `foundry_config::Config` for the current workspace (see
    /// [`utils::find_project_root_path`] and merges the cli `BuildArgs` into it before returning
    /// [`foundry_config::Config::project()`]
    pub fn project(&self) -> eyre::Result<Project> {
        self.args.project()
    }

    /// Returns whether `ZkBuildArgs` was configured with `--watch`
    pub fn is_watch(&self) -> bool {
        self.watch.watch.is_some()
    }
    /// Returns the [`watchexec::InitConfig`] and [`watchexec::RuntimeConfig`] necessary to
    /// bootstrap a new [`watchexe::Watchexec`] loop.
    pub fn watchexec_config(&self) -> eyre::Result<(InitConfig, RuntimeConfig)> {
        // use the path arguments or if none where provided the `src` dir
        self.watch.watchexec_config(|| {
            let config = Config::from(self);
            vec![config.src, config.test, config.script]
        })
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
        let mut dict = value.into_dict().ok_or(error)?;

        if self.names {
            dict.insert("names".to_string(), true.into());
        }

        if self.sizes {
            dict.insert("sizes".to_string(), true.into());
        }

        Ok(Map::from([(Config::selected_profile(), dict)]))
    }
}
