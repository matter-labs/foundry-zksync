//! # foundry-strategy-zksync
//!
//! Strategies for ZKsync network.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

mod backend;
mod cheatcode;
mod executor;

pub use backend::ZksyncBackendStrategy;
pub use cheatcode::ZksyncCheatcodeInspectorStrategy;
pub use executor::{get_zksync_transaction_metadata, new_zkysnc_strategy, ZksyncExecutorStrategy};
