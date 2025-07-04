use foundry_compilers::ProjectCompileOutput;
use foundry_evm::executors::strategy::ExecutorStrategyContext;
use foundry_zksync_compilers::{
    compilers::{artifact_output::zk::ZkArtifactOutput, zksolc::ZkSolcCompiler},
    dual_compiled_contracts::DualCompiledContracts,
};
use foundry_zksync_core::{vm::ZkEnv, ZkTransactionMetadata};

/// Defines the context for [ZksyncExecutorStrategyRunner].
#[derive(Debug, Default, Clone)]
pub struct ZksyncExecutorStrategyContext {
    pub(super) transaction_context: Option<ZkTransactionMetadata>,
    pub(super) compilation_output: Option<ProjectCompileOutput<ZkSolcCompiler, ZkArtifactOutput>>,
    pub(super) dual_compiled_contracts: DualCompiledContracts,
    pub(super) zk_env: ZkEnv,
}

impl ExecutorStrategyContext for ZksyncExecutorStrategyContext {
    fn new_cloned(&self) -> Box<dyn ExecutorStrategyContext> {
        Box::new(self.clone())
    }

    fn as_any_ref(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
