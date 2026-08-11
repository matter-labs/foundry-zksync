//! # foundry-zksync
//!
//! Foundry ZKSync compiler data structures and trait implementations.
#![warn(missing_docs, unused_crate_dependencies)]

pub mod artifacts;
pub mod compilers;
pub mod dual_compiled_contracts;
pub mod link;

// TODO: Used in integration tests.
// find out why cargo complains about unused dev_dependency for these cases
#[cfg(test)]
use foundry_test_utils as _;
#[cfg(test)]
use tempfile as _;
