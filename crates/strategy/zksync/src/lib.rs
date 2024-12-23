//! # foundry-strategy-zksync
//!
//! Strategies for ZKsync network.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

mod backend;
mod cheatcode;
mod executor;

pub use backend::ZksyncBackendStrategyRunner;
pub use cheatcode::ZksyncCheatcodeInspectorStrategyRunner;
pub use executor::{
    try_get_zksync_transaction_metadata, ZksyncExecutorStrategyBuilder,
    ZksyncExecutorStrategyRunner,
};
