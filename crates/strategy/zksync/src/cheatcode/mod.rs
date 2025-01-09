use foundry_cheatcodes::strategy::CheatcodeInspectorStrategy;
use foundry_zksync_compilers::dual_compiled_contracts::DualCompiledContracts;
use foundry_zksync_core::vm::ZkEnv;

mod context;
mod runner;
pub(crate) mod types;

pub use self::{
    context::ZksyncCheatcodeInspectorStrategyContext,
    runner::ZksyncCheatcodeInspectorStrategyRunner,
};

/// Create ZKsync strategy for [CheatcodeInspectorStrategy].
pub trait ZksyncCheatcodeInspectorStrategyBuilder {
    /// Create new ZKsync strategy.
    fn new_zksync(dual_compiled_contracts: DualCompiledContracts, zk_env: ZkEnv) -> Self;
}

impl ZksyncCheatcodeInspectorStrategyBuilder for CheatcodeInspectorStrategy {
    fn new_zksync(dual_compiled_contracts: DualCompiledContracts, zk_env: ZkEnv) -> Self {
        Self {
            runner: &ZksyncCheatcodeInspectorStrategyRunner,
            context: Box::new(ZksyncCheatcodeInspectorStrategyContext::new(
                dual_compiled_contracts,
                zk_env,
            )),
        }
    }
}
