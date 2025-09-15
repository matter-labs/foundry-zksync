use std::any::Any;

use alloy_primitives::{Address, U256, map::HashMap};
use foundry_evm::backend::strategy::BackendStrategyContext;
use foundry_zksync_core::{PaymasterParams, vm::ZkEnv};
use revm::primitives::HashSet;
use zksync_types::H256;

/// Context for [ZksyncBackendStrategyRunner].
#[derive(Debug, Default, Clone)]
pub struct ZksyncBackendStrategyContext {
    /// Store storage keys per contract address for immutable variables.
    pub(super) persistent_immutable_keys: HashMap<Address, HashSet<U256>>,
    /// Store persisted factory dependencies.
    pub(super) persisted_factory_deps: HashMap<H256, Vec<u8>>,
    /// Whether to use EVM interpreter
    #[allow(dead_code)]
    pub(super) evm_interpreter: bool,
}

impl BackendStrategyContext for ZksyncBackendStrategyContext {
    fn new_cloned(&self) -> Box<dyn BackendStrategyContext> {
        Box::new(self.clone())
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Represents additional data for ZK transactions.
///
/// Not exposed publicly, only set within the strategy.
#[derive(Clone, Debug, Default)]
pub(crate) struct ZksyncInspectContext {
    /// Factory Deps for ZK transactions.
    pub factory_deps: Vec<Vec<u8>>,
    /// Paymaster data for ZK transactions.
    pub paymaster_data: Option<PaymasterParams>,
    /// Zksync environment.
    pub zk_env: ZkEnv,
    /// Use EVM interpreter.
    pub  evm_interpreter: bool,
}
