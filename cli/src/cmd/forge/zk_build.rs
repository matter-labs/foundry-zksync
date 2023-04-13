use tokio;
use tokio::sync::oneshot;
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

        // let (tx, mut rx) = oneshot::channel();
        //     tokio::spawn(async move {
        //     let result = zksolc_manager.clone().download().await;
        //     let _ = tx.send(result);
        // });
        // match rx.try_recv() {
        //     Ok(result) => {
        //         println!("Downloaded");
        //     }
        //     Err(e) => {
        //         println!("Error: {}", e);
        //     }
        // }

        // let download = tokio::runtime::Builder::new_current_thread()
        //     .enable_all()
        //     .build()
        //     .unwrap()
        //     .block_on(zksolc_manager.clone().download());

        // println!("{}", zksolc_manager);
        
        Ok("".to_owned())
    }
}
