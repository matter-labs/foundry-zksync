mod backend;
mod cheatcode;
mod executor;

pub use backend::{get_zksync_transaction_metadata, ZksyncBackendStrategy};
pub use cheatcode::ZksyncCheatcodeInspectorStrategy;
pub use executor::ZksyncExecutorStrategy;

// #[derive(Debug, Default, Clone)]
// pub struct ZksyncStrategy;

// impl GlobalStrategy for ZksyncStrategy {
//     type Backend = ZkBackendStrategy;
//     type Executor = ZkExecutor;
//     type CheatcodeInspector = ZkCheatcodeInspector;
// }

// pub struct ZkRunnerStrategy {
//     pub backend: Arc<Mutex<ZksyncBackendStrategy>>,
// }
// impl Default for ZkRunnerStrategy {
//     fn default() -> Self {
//         Self { backend: Arc::new(Mutex::new(ZksyncBackendStrategy::default())) }
//     }
// }
// impl RunnerStrategy for ZkRunnerStrategy {
//     fn name(&self) -> &'static str {
//         "zk"
//     }

//     fn backend_strategy(&self) -> Arc<Mutex<impl BackendStrategy>> {
//         self.backend.clone()
//     }
// }
