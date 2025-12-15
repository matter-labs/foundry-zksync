//! # foundry-evm-core
//!
//! Core EVM abstractions.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg))]

use crate::constants::DEFAULT_CREATE2_DEPLOYER;
use alloy_evm::eth::EthEvmContext;
use alloy_primitives::{Address, map::HashMap};
use auto_impl::auto_impl;
use backend::DatabaseExt;
use revm::{
    Inspector,
    inspector::NoOpInspector,
    interpreter::{CallInputs, CreateInputs},
};
use revm_inspectors::access_list::AccessListInspector;

/// Map keyed by breakpoints char to their location (contract address, pc)
pub type Breakpoints = HashMap<char, (Address, usize)>;

#[macro_use]
extern crate tracing;

pub mod abi {
    pub use foundry_cheatcodes_spec::Vm;
    pub use foundry_evm_abi::*;
}

pub mod env;
pub use env::*;
use foundry_evm_networks::NetworkConfigs;

pub mod backend;
pub mod buffer;
pub mod bytecode;
pub mod constants;
pub mod decode;
pub mod either_evm;
pub mod evm;
pub mod fork;
pub mod hardfork;
pub mod ic;
pub mod opts;
pub mod precompiles;
pub mod state_snapshot;
pub mod utils;

pub type Ecx<'a, 'b, 'c> = &'a mut EthEvmContext<&'b mut (dyn DatabaseExt + 'c)>;

/// An extension trait that allows us to add additional hooks to Inspector for later use in
/// handlers.
#[auto_impl(&mut, Box)]
pub trait InspectorExt: for<'a> Inspector<EthEvmContext<&'a mut dyn DatabaseExt>> {
    /// Determines whether the `DEFAULT_CREATE2_DEPLOYER` should be used for a CREATE2 frame.
    ///
    /// If this function returns true, we'll replace CREATE2 frame with a CALL frame to CREATE2
    /// factory.
    fn should_use_create2_factory(
        &mut self,
        _context: &mut EthEvmContext<&mut dyn DatabaseExt>,
        _inputs: &CreateInputs,
    ) -> bool {
        false
    }

    /// Simulates `console.log` invocation.
    fn console_log(&mut self, msg: &str) {
        let _ = msg;
    }

    /// Returns configured networks.
    fn get_networks(&self) -> NetworkConfigs {
        NetworkConfigs::default()
    }

    /// Returns the CREATE2 deployer address.
    fn create2_deployer(&self) -> Address {
        DEFAULT_CREATE2_DEPLOYER
    }

    fn zksync_set_deployer_call_input(
        &mut self,
        _context: Ecx<'_, '_, '_>,
        _call_inputs: &mut CallInputs,
    ) {
        // No-op by default, can be overridden in the inspector.
    }

    /// Appends provided zksync traces.
    // TODO(merge): Should be moved outside of the upstream codebase
    fn trace_zksync(
        &mut self,
        _context: Ecx<'_, '_, '_>,
        _call_traces: Box<dyn std::any::Any>, // holds `Vec<Call>`
        _record_top_call: bool,
    ) {
    }
}

impl InspectorExt for NoOpInspector {}

impl InspectorExt for AccessListInspector {}
