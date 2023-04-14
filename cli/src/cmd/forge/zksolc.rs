use std::process::Command;
use anyhow::{Result, Error};
use serde::Serialize;
use std::path::PathBuf;
use foundry_config::Config;

#[derive(Debug, Clone, Serialize)]
pub struct ZkSolcOpts<'a> {
    pub config: &'a Config,
    pub path: PathBuf,
    pub is_system: bool,
    pub force_evmla: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ZkSolc {
    // pub config: &'a Config,
    pub path: PathBuf,
    pub is_system: bool,
    pub force_evmla: bool,
}

impl ZkSolc {
    pub fn new(opts: ZkSolcOpts) -> Self {
        Self {
            path: opts.path,
            // config: todo!(),
            is_system: todo!(),
            force_evmla: todo!(),
        }
    }
    
    pub fn compile(self) -> Result<()> {
        let binary = self.path;

        let output = Command::new(binary).output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            println!("Output: {}", stdout);
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("Error: {}", stderr);
        }

        Ok(())
    }
}
