//! # foundry-cheatcodes
//!
//! Foundry cheatcodes implementations.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![allow(elided_lifetimes_in_paths)] // Cheats context uses 3 lifetimes

#[macro_use]
extern crate foundry_common;

#[macro_use]
pub extern crate foundry_cheatcodes_spec as spec;

#[macro_use]
extern crate tracing;

use alloy_evm::eth::EthEvmContext;
use alloy_primitives::Address;
use foundry_evm_core::backend::DatabaseExt;
use revm::context::{ContextTr, JournalTr};
use spec::Status;

/// The inner EVM context type (without the outer `&mut`), suitable as a generic
/// parameter for `CheatsCtxt<'_, CTX>` where `CTX: CheatsCtxExt`.
pub type ConcreteEcx<'b, 'c> = EthEvmContext<&'b mut (dyn DatabaseExt + 'c)>;

pub use Vm::ForgeContext;
pub use config::CheatsConfig;
pub use error::{Error, ErrorKind, Result};
pub use inspector::{
    BroadcastableTransaction, BroadcastableTransactions, Cheatcodes, CheatcodesExecutor,
    CheatsCtxExt, NestedEvmClosure,
};
pub use spec::{CheatcodeDef, Vm};

// Note(zk): Exposed for ZKsync usage.
pub use evm::{journaled_account, mock::make_acc_non_empty};

#[macro_use]
mod error;

mod base64;

mod config;

mod crypto;

mod version;

mod env;
pub use env::set_execution_context;

mod evm;
pub use evm::{DealRecord, mock::mock_call};

mod fs;

mod inspector;
pub use inspector::{CheatcodeAnalysis, CommonCreateInput};

mod json;

mod script;
pub use script::{Broadcast, Wallets, WalletsInner};

mod string;

mod test;
pub use test::expect::handle_expect_emit;

mod toml;

mod utils;

pub mod strategy;

/// Cheatcode implementation.
pub(crate) trait Cheatcode: CheatcodeDef {
    /// Applies this cheatcode to the given state.
    ///
    /// Implement this function if you don't need access to the EVM data.
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let _ = state;
        unimplemented!("{}", Self::CHEATCODE.func.id)
    }

    /// Applies this cheatcode to the given context.
    ///
    /// Implement this function if you need access to the EVM data.
    #[inline(always)]
    fn apply_stateful<CTX: CheatsCtxExt>(&self, ccx: &mut CheatsCtxt<'_, CTX>) -> Result {
        self.apply(ccx.state)
    }

    /// Applies this cheatcode to the given context and executor.
    ///
    /// Implement this function if you need access to the executor.
    #[inline(always)]
    fn apply_full<CTX: CheatsCtxExt>(
        &self,
        ccx: &mut CheatsCtxt<'_, CTX>,
        executor: &mut dyn CheatcodesExecutor<CTX>,
    ) -> Result {
        let _ = executor;
        self.apply_stateful(ccx)
    }
}

pub trait DynCheatcode: 'static + std::fmt::Debug {
    fn cheatcode(&self) -> &'static spec::Cheatcode<'static>;

    fn dyn_apply<'b, 'c>(
        &self,
        ccx: &mut CheatsCtxt<'_, ConcreteEcx<'b, 'c>>,
        executor: &mut dyn CheatcodesExecutor<ConcreteEcx<'b, 'c>>,
    ) -> Result;

    fn as_any(&self) -> &dyn std::any::Any;
}

impl<T: Cheatcode + 'static> DynCheatcode for T {
    fn cheatcode(&self) -> &'static spec::Cheatcode<'static> {
        Self::CHEATCODE
    }

    fn dyn_apply<'b, 'c>(
        &self,
        ccx: &mut CheatsCtxt<'_, ConcreteEcx<'b, 'c>>,
        executor: &mut dyn CheatcodesExecutor<ConcreteEcx<'b, 'c>>,
    ) -> Result {
        self.apply_full(ccx, executor)
    }

    #[inline]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl dyn DynCheatcode {
    pub(crate) fn name(&self) -> &'static str {
        self.cheatcode().func.signature.split('(').next().unwrap()
    }

    pub(crate) fn id(&self) -> &'static str {
        self.cheatcode().func.id
    }

    pub(crate) fn signature(&self) -> &'static str {
        self.cheatcode().func.signature
    }

    pub(crate) fn status(&self) -> &Status<'static> {
        &self.cheatcode().status
    }
}

/// The cheatcode context.
pub struct CheatsCtxt<'a, CTX> {
    /// The cheatcodes inspector state.
    pub state: &'a mut Cheatcodes,
    /// The EVM context.
    pub ecx: &'a mut CTX,
    /// The original `msg.sender`.
    pub caller: Address,
    /// Gas limit of the current cheatcode call.
    pub gas_limit: u64,
}

impl<CTX> std::ops::Deref for CheatsCtxt<'_, CTX> {
    type Target = CTX;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.ecx
    }
}

impl<CTX> std::ops::DerefMut for CheatsCtxt<'_, CTX> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ecx
    }
}

impl<CTX: ContextTr> CheatsCtxt<'_, CTX> {
    pub(crate) fn ensure_not_precompile(&self, address: &Address) -> Result<()> {
        if self.is_precompile(address) { Err(precompile_error(address)) } else { Ok(()) }
    }

    pub(crate) fn is_precompile(&self, address: &Address) -> bool {
        self.ecx.journal().precompile_addresses().contains(address)
    }
}

#[cold]
fn precompile_error(address: &Address) -> Error {
    fmt_err!("cannot use precompile {address} as an argument")
}
