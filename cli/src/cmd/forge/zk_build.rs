use tokio;
use tokio::sync::oneshot;
use std::time::Duration;
use std::thread::sleep;
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
            version: "v1.3.8".to_owned(),
        };
        
        let zksolc_manager_builder = ZkSolcManagerBuilder::new(zksolc_manager_opts); 
        let zksolc_manager = zksolc_manager_builder.build().unwrap();

        match zksolc_manager.download() {
            Ok(_) => println!("File downloaded successfully."),
            Err(e) => println!("Failed to download the file: {}", e),
        }
        
        Ok("".to_owned())
    }
}
