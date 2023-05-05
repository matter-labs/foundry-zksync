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
        default_value = Some("v1.3.9")
    )]
    #[serde(skip)]
    pub use_zksolc: Option<String>,

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

    fn run(self) -> eyre::Result<()> {
        let config = self.try_load_config_emit_warnings()?;
        let mut project = config.project()?;

        //set zk out path
        let zk_out_path = project.paths.root.join("zkout");
        project.paths.artifacts = zk_out_path;

        let zksolc_manager_opts = ZkSolcManagerOpts { version: self.use_zksolc.unwrap() };
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
