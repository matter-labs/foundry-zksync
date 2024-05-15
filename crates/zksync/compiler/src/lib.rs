//! # foundry-zksync
//!
//! Main Foundry ZKSync implementation.
#![warn(missing_docs, unused_crate_dependencies)]

/// ZKSolc specific logic.
mod zksolc;

pub use zksolc::*;

pub mod libraries;
