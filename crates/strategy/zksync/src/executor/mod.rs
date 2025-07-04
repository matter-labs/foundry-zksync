use foundry_evm::executors::strategy::ExecutorStrategy;

mod context;
mod runner;
mod utils;

pub use self::{
    context::ZksyncExecutorStrategyContext, runner::ZksyncExecutorStrategyRunner,
    utils::try_get_zksync_transaction_metadata,
};

/// Create ZKsync strategy for [ExecutorStrategy].
pub trait ZksyncExecutorStrategyBuilder {
    /// Create new zksync strategy.
    fn new_zksync() -> Self;
}

impl ZksyncExecutorStrategyBuilder for ExecutorStrategy {
    fn new_zksync() -> Self {
        Self {
            runner: &ZksyncExecutorStrategyRunner,
            context: Box::new(ZksyncExecutorStrategyContext::default()),
        }
    }
}
