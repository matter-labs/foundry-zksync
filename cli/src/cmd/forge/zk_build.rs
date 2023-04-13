use clap::Parser;
use serde::Serialize;
use crate::cmd::{
    Cmd,
};

use super::zksolc_manager::{ZkSolcManagerOpts, ZkSolcManagerBuilder};

#[derive(Debug, Clone, Parser, Serialize)]
#[clap(next_help_heading = "ZkBuild options", about = None)] 
pub struct ZkBuildArgs {}

impl Cmd for ZkBuildArgs {
    type Output = String;
    fn run(self) -> eyre::Result<String> {
        let zksolc_manager_opts = ZkSolcManagerOpts {
            version: "v0.0.0".to_owned(),
        };
        
        let zksolc_manager_builder = ZkSolcManagerBuilder::new(zksolc_manager_opts); 

        let zksolc_manager = zksolc_manager_builder.build().unwrap();

        Ok("".to_owned())
    }
}
