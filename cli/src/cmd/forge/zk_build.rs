use clap::Parser;
use serde::Serialize;
use crate::cmd::{
    Cmd,
};
use super::zksolc_manager::{ZkSolcManagerOpts, ZkSolcManagerBuilder};
use super::zksolc::{ZkSolcOpts, ZkSolc};

#[derive(Debug, Clone, Parser, Serialize, Default)]
#[clap(next_help_heading = "ZkBuild options", about = None)] 
pub struct ZkBuildArgs {
    /// Specify the solc version, or a path to a local solc, to build with.
    ///
    /// Valid values are in the format `x.y.z`, `solc:x.y.z` or `path/to/solc`.
    #[clap(help_heading = "Compiler options", value_name = "ZK_SOLC_VERSION", long = "use_zksolc")]
    #[serde(skip)]
    pub use_zksolc: Option<String>,
}

impl Cmd for ZkBuildArgs {
    type Output = String;

    fn run(self) -> eyre::Result<String> {
        // let mut config = self.try_load_config_emit_warnings()?;
        // let mut project = config.project()?;

        let zksolc_manager_opts = ZkSolcManagerOpts {
            version: self.use_zksolc.unwrap(),
        };
        
        let zksolc_manager_builder = ZkSolcManagerBuilder::new(zksolc_manager_opts); 
        let zksolc_manager = zksolc_manager_builder.build();

        match zksolc_manager {
            Ok(zksolc_manager) => {
                if !zksolc_manager.exists() {
                    println!("Downloading zksolc compiler");
                    
                    match zksolc_manager.clone().download() {
                        Ok(zksolc_manager) => zksolc_manager,
                        Err(e) => println!("Failed to download the file: {}", e),
                    }
                }

                println!("Compiling smart contracts");

                let zksolc_opts = ZkSolcOpts {
                    path: zksolc_manager.get_full_compiler_path(),
                    config: todo!(),
                    is_system: todo!(),
                    force_evmla: todo!(),
                };

                let zksolc = ZkSolc::new(zksolc_opts);

                zksolc.compile();
            },
            Err(e) => println!("Error building zksolc_manager: {}", e),
        }
        
        Ok("".to_owned())
    }
}
