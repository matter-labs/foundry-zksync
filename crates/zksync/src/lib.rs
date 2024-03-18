//! # foundry-zksync
//!
//! Main Foundry ZKSync implementation.
#![warn(missing_docs, unused_crate_dependencies)]

/// Contains cheatcode implementations.
pub mod cheatcodes;

/// Contains conversion utils for revm primitives.
pub mod convert;

/// Contains zksync utils.
pub mod utils;

/// ZKSync Era VM implementation.
pub mod vm;

/// ZKSolc specific logic.
pub mod zksolc;

pub use utils::{fix_l2_gas_limit, fix_l2_gas_price};
pub use vm::{balance, encode_create_params, nonce};
pub use zksolc::DualCompiledContract;

/// Represents additional data for ZK transactions.
#[derive(Clone, Debug, Default)]
pub struct ZkTransactionMetadata {
    /// Factory Deps for ZK transactions.
    pub factory_deps: Vec<Vec<u8>>,
}
