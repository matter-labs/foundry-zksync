use foundry_evm::backend::strategy::BackendStrategy;

mod context;
mod merge;
mod runner;

pub(crate) use self::context::ZksyncInspectContext;
pub use self::{context::ZksyncBackendStrategyContext, runner::ZksyncBackendStrategyRunner};

/// Create ZKsync strategy for [BackendStrategy].
pub trait ZksyncBackendStrategyBuilder {
    /// Create new zksync strategy.
    fn new_zksync(evm_interpreter: bool) -> Self;
}

impl ZksyncBackendStrategyBuilder for BackendStrategy {
    fn new_zksync(evm_interpreter: bool) -> Self {
        Self {
            runner: &ZksyncBackendStrategyRunner,
            context: Box::new(ZksyncBackendStrategyContext{
                evm_interpreter,
                ..Default::default()
            }),
        }
    }
}
