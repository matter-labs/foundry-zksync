//! # foundry-zksync
//!
//! Foundry ZKSync inspectors implementations.
#![warn(missing_docs, unused_crate_dependencies)]

mod trace;
pub use trace::TraceCollector;

pub use foundry_evm_traces as traces;
