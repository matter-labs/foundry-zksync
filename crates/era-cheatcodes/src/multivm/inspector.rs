use foundry_common::{
    AsTracerPointer, EnvironmentTracker, StorageModificationRecorder, StorageModifications,
};
use foundry_evm_core::backend::DatabaseExt;
use multivm::{
    interface::dyn_tracers::vm_1_4_0::DynTracer,
    vm_latest::{HistoryMode, SimpleMemory, VmTracer},
};
use revm::{primitives::Env, Inspector};
use zksync_state::WriteStorage;

#[derive(Clone, Debug)]
pub struct NoopInspector;

impl<DB: DatabaseExt> Inspector<DB> for NoopInspector {}

impl<S: WriteStorage, H: HistoryMode> AsTracerPointer<S, H> for NoopInspector {
    fn as_tracer_pointer(&self) -> multivm::vm_latest::TracerPointer<S, H> {
        Box::new(NoopInspector)
    }
}

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for NoopInspector {}
impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for NoopInspector {}

impl StorageModificationRecorder for NoopInspector {
    fn record_storage_modifications(&mut self, _storage_modifications: StorageModifications) {}

    fn get_storage_modifications(&self) -> &StorageModifications {
        let mods = Box::<StorageModifications>::default();
        Box::leak(mods)
    }
}

impl EnvironmentTracker for NoopInspector {
    fn record_environment(&mut self, _environment: Env) {}

    fn get_environment(&self) -> &Env {
        let env = Box::<Env>::default();
        Box::leak(env)
    }
}
