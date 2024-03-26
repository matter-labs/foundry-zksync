use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use alloy_primitives::{hex, Address, Bytes, U256 as rU256};
use foundry_cheatcodes_common::{
    expect::ExpectedCallTracker,
    mock::{MockCallDataContext, MockCallReturnData},
};
use multivm::{
    interface::{dyn_tracers::vm_1_4_1::DynTracer, tracer::TracerExecutionStatus},
    vm_latest::{BootloaderState, HistoryMode, SimpleMemory, VmTracer, ZkSyncVmState},
    zk_evm_latest::{
        tracing::{AfterDecodingData, AfterExecutionData, BeforeExecutionData, VmLocalStateData},
        zkevm_opcode_defs::{FatPointer, Opcode, CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER},
    },
};
use once_cell::sync::OnceCell;
use zksync_state::WriteStorage;
use zksync_types::{H256, U256};

use crate::convert::{ConvertH160, ConvertH256, ConvertU256};

use super::farcall::FarCallHandler;

/// extendedAccountVersion(address)
const SELECTOR_ACCOUNT_VERSION: [u8; 4] = hex!("bb0fd610");

/// Represents the context for [CheatcodeContext]
#[derive(Debug, Default)]
pub struct CheatcodeTracerContext<'a> {
    /// Mocked calls.
    pub mocked_calls: HashMap<Address, BTreeMap<MockCallDataContext, MockCallReturnData>>,
    /// Expected calls recorder.
    pub expected_calls: Option<&'a mut ExpectedCallTracker>,
}

#[derive(Debug, Default)]
pub struct CheatcodeTracerResult {
    pub expected_calls: ExpectedCallTracker,
}

#[derive(Debug, Default)]
pub struct CheatcodeTracer {
    pub farcall_handler: FarCallHandler,
    pub mocked_calls: HashMap<Address, BTreeMap<MockCallDataContext, MockCallReturnData>>,
    pub expected_calls: ExpectedCallTracker,
    pub caller: Address,
    pub result: Arc<OnceCell<CheatcodeTracerResult>>,
}

impl<S: Send, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for CheatcodeTracer {
    fn before_decoding(&mut self, _state: VmLocalStateData<'_>, _memory: &SimpleMemory<H>) {}

    fn after_decoding(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: AfterDecodingData,
        _memory: &SimpleMemory<H>,
    ) {
    }

    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        memory: &SimpleMemory<H>,
        storage: zksync_state::StoragePtr<S>,
    ) {
        self.farcall_handler.track_active_far_calls(state, data, memory, storage);
    }

    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterExecutionData,
        memory: &SimpleMemory<H>,
        _storage: zksync_state::StoragePtr<S>,
    ) {
        // Mark the caller as EOA to avoid panic
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let calldata = get_calldata(&state, memory);

            if calldata.starts_with(&SELECTOR_ACCOUNT_VERSION) {
                let address = H256::from_slice(&calldata[4..36]).to_h160().to_address();
                if self.caller == address {
                    let mut bytes = [0u8; 32];
                    U256::one().to_big_endian(&mut bytes);
                    tracing::debug!(
                        "overriding account version {:?} for caller {address:?}",
                        hex::encode(&bytes)
                    );
                    self.farcall_handler.set_immediate_return(bytes.to_vec());
                    return
                }
            }
        }

        // Checks contract calls for expectCall cheatcode
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let current = state.vm_local_state.callstack.current;
            if let Some(expected_calls_for_target) =
                self.expected_calls.get_mut(&current.code_address.to_address())
            {
                let calldata = get_calldata(&state, memory);
                // Match every partial/full calldata
                for (expected_calldata, (expected, actual_count)) in expected_calls_for_target {
                    // Increment actual times seen if...
                    // The calldata is at most, as big as this call's input, and
                    if expected_calldata.len() <= calldata.len() &&
                    // Both calldata match, taking the length of the assumed smaller one (which will have at least the selector), and
                    *expected_calldata == calldata[..expected_calldata.len()] &&
                    // The value matches, if provided
                    expected
                        .value
                        .map_or(true, |value|{
                             value == rU256::from(current.context_u128_value)})
                    {
                        *actual_count += 1;
                    }
                }
            }
        }

        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let current = state.vm_local_state.callstack.current;
            let call_input = get_calldata(&state, memory);
            let call_contract = current.code_address.to_address();
            let call_value = U256::from(current.context_u128_value).to_ru256();
            // Handle mocked calls
            if let Some(mocks) = self.mocked_calls.get(&call_contract) {
                let ctx = MockCallDataContext {
                    calldata: Bytes::from(call_input.clone()),
                    value: Some(call_value),
                };
                if let Some(return_data) = mocks.get(&ctx).or_else(|| {
                    mocks
                        .iter()
                        .find(|(mock, _)| {
                            call_input.get(..mock.calldata.len()) == Some(&mock.calldata[..]) &&
                                mock.value.map_or(true, |value| value == call_value)
                        })
                        .map(|(_, v)| v)
                }) {
                    let return_data = return_data.data.clone().to_vec();
                    tracing::debug!("returning mocked value {:?}", hex::encode(&return_data));
                    self.farcall_handler.set_immediate_return(return_data);
                }
            }
        }
    }
}

impl<S: WriteStorage + Send, H: HistoryMode> VmTracer<S, H> for CheatcodeTracer {
    fn initialize_tracer(&mut self, _state: &mut ZkSyncVmState<S, H>) {}

    fn finish_cycle(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        self.farcall_handler.maybe_return_early(state, bootloader_state);

        TracerExecutionStatus::Continue
    }

    fn after_vm_execution(
        &mut self,
        _state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &BootloaderState,
        _stop_reason: multivm::interface::tracer::VmExecutionStopReason,
    ) {
        let cell = self.result.as_ref();
        cell.set(CheatcodeTracerResult { expected_calls: self.expected_calls.clone() }).unwrap();
    }
}
fn get_calldata<H: HistoryMode>(state: &VmLocalStateData<'_>, memory: &SimpleMemory<H>) -> Vec<u8> {
    let ptr = state.vm_local_state.registers[CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER as usize];
    assert!(ptr.is_pointer);
    let fat_data_pointer = FatPointer::from_u256(ptr.value);
    memory.read_unaligned_bytes(
        fat_data_pointer.memory_page as usize,
        fat_data_pointer.start as usize,
        fat_data_pointer.length as usize,
    )
}
