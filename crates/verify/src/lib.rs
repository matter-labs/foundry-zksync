//! Smart contract verification.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

#[macro_use]
extern crate foundry_common;

#[macro_use]
extern crate tracing;

mod etherscan;

pub mod provider;
pub mod zk_provider;

pub mod bytecode;
pub mod retry;
mod sourcify;
mod zksync;
pub use bytecode::VerifyBytecodeArgs;

pub use retry::RetryArgs;

pub mod verify;
pub use verify::{VerifierArgs, VerifyArgs, VerifyCheckArgs};

mod types;

mod utils;
