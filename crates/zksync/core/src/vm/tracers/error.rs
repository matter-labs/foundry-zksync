use zksync_multivm::{
    tracers::dynamic::vm_1_5_0::DynTracer,
    vm_latest::{HistoryMode, SimpleMemory, VmTracer},
    zk_evm_latest::{
        tracing::{AfterDecodingData, VmLocalStateData},
        vm_state::ErrorFlags,
    },
};
use zksync_state::interface::{ReadStorage, WriteStorage};

/// A tracer to allow logging low-level vm errors.
#[derive(Debug, Default)]
pub struct ErrorTracer;

impl<S: ReadStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for ErrorTracer {
    fn after_decoding(
        &mut self,
        _state: VmLocalStateData<'_>,
        data: AfterDecodingData,
        _memory: &SimpleMemory<H>,
    ) {
        if data.error_flags_accumulated.is_empty() {
            return;
        }

        let errors = parse_error_flags(&data.error_flags_accumulated);
        tracing::error!("vm error: {}", errors.join(", "));
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for ErrorTracer {}

fn parse_error_flags(error_flags: &ErrorFlags) -> Vec<String> {
    let mut errors = vec![];
    if error_flags.contains(ErrorFlags::INVALID_OPCODE) {
        errors.push(String::from("Invalid opcode"));
    }
    if error_flags.contains(ErrorFlags::NOT_ENOUGH_ERGS) {
        errors.push(String::from("Not enough gas"));
    }
    if error_flags.contains(ErrorFlags::PRIVILAGED_ACCESS_NOT_FROM_KERNEL) {
        errors.push(String::from("Unauthorized privileged access"));
    }
    if error_flags.contains(ErrorFlags::WRITE_IN_STATIC_CONTEXT) {
        errors.push(String::from("Write applied in static context"));
    }
    if error_flags.contains(ErrorFlags::CALLSTACK_IS_FULL) {
        errors.push(String::from("Call stack full"));
    }
    errors
}
