use std::sync::{Arc, Mutex};

use foundry_evm_core::backend::strategy::{BackendStrategy, EvmBackendStrategy};
pub trait RunnerStrategy: Send + Sync {
    fn name(&self) -> &'static str;
    fn backend_strategy(&self) -> Arc<Mutex<dyn BackendStrategy>>;
}

pub struct EvmRunnerStrategy {
    pub backend: Arc<Mutex<dyn BackendStrategy>>,
}
impl Default for EvmRunnerStrategy {
    fn default() -> Self {
        Self { backend: Arc::new(Mutex::new(EvmBackendStrategy)) }
    }
}
impl RunnerStrategy for EvmRunnerStrategy {
    fn name(&self) -> &'static str {
        "evm"
    }

    fn backend_strategy(&self) -> Arc<Mutex<dyn BackendStrategy>> {
        self.backend.clone()
    }
}
