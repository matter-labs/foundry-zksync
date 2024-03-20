use std::collections::{BTreeMap, HashMap};

use alloy_primitives::{Address, Bytes, U256 as rU256};
use foundry_cheatcodes_common::{
    expect::ExpectedCallTracker,
    mock::{MockCallDataContext, MockCallReturnData},
};
use multivm::{
    interface::{dyn_tracers::vm_1_4_1::DynTracer, tracer::TracerExecutionStatus, VmRevertReason},
    vm_latest::{
        BootloaderState, HistoryMode, L1BatchEnv, SimpleMemory, SystemEnv, VmTracer, ZkSyncVmState,
    },
    zk_evm_latest::{
        tracing::{AfterDecodingData, AfterExecutionData, BeforeExecutionData, VmLocalStateData},
        zkevm_opcode_defs::{FatPointer, Opcode, CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER},
    },
};
use revm::interpreter::InstructionResult;
use zksync_state::WriteStorage;
use zksync_types::{H160, U256};

use crate::convert::{ConvertH160, ConvertU256};

use super::farcall::{FarCallHandler, MockedCalls};

// #[derive(Debug, Clone)]
// struct ExpectedCallData {
//     /// The expected value sent in the call
//     value: Option<U256>,
//     /// The number of times the call is expected to be made.
//     /// If the type of call is `NonCount`, this is the lower bound for the number of calls
//     /// that must be seen.
//     /// If the type of call is `Count`, this is the exact number of calls that must be seen.
//     count: u64,
//     /// The type of expected call.
//     call_type: ExpectedCallType,
// }

// /// The type of expected call.
// #[derive(Clone, Debug, PartialEq, Eq)]
// enum ExpectedCallType {
//     /// The call is expected to be made at least once.
//     NonCount,
//     /// The exact number of calls expected.
//     Count,
// }

// /// Tracks the expected calls per address.
// ///
// /// For each address, we track the expected calls per call data. We track it in such manner
// /// so that we don't mix together calldatas that only contain selectors and calldatas that
// contain /// selector and arguments (partial and full matches).
// ///
// /// This then allows us to customize the matching behavior for each call data on the
// /// `ExpectedCallData` struct and track how many times we've actually seen the call on the second
// /// element of the tuple.
// type ExpectedCallsTracker = HashMap<H160, HashMap<Vec<u8>, (ExpectedCallData, u64)>>;

#[derive(Debug, Default)]
pub struct CheatcodeTracerContext {
    pub mocked_calls: HashMap<Address, BTreeMap<MockCallDataContext, MockCallReturnData>>,
    // pub mocked_calls: MockedCalls,
}

#[derive(Debug, Default)]
pub struct CheatcodeTracer {
    pub farcall_handler: FarCallHandler,
    pub mocked_calls: HashMap<Address, BTreeMap<MockCallDataContext, MockCallReturnData>>,
    pub expected_calls: ExpectedCallTracker,
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
        storage: zksync_state::StoragePtr<S>,
    ) {
        // println!("{:?}", data.opcode);
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let current = state.vm_local_state.callstack.current;
            let call_input = get_calldata(&state, memory);
            let call_contract = current.code_address.to_address();
            let call_value = U256::from(current.context_u128_value).to_ru256();
            // println!("FAR {:?} {call_value:?} = {:?}", call_contract, hex::encode(&call_input));
        }

        if let Opcode::NearCall(_call) = data.opcode.variant.opcode {
            let current = state.vm_local_state.callstack.current;
            // let call_input = get_calldata(&state, memory);
            let call_contract = current.code_address.to_address();
            // let call_value = U256::from(current.context_u128_value).to_ru256();
            // println!("NEAR {:?} ", call_contract);
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
                println!("\tgot mock for {call_contract:?}");
                let ctx = MockCallDataContext {
                    calldata: Bytes::from(call_input.clone()),
                    value: Some(call_value),
                };
                if let Some(return_data) = mocks.get(&ctx).or_else(|| {
                    mocks
                        .iter()
                        .find(|(mock, _)| {
                            println!("\t\t\tsearch {:?}", mock);
                            call_input.get(..mock.calldata.len()) == Some(&mock.calldata[..]) &&
                                mock.value.map_or(true, |value| value == call_value)
                        })
                        .map(|(_, v)| v)
                }) {
                    println!("\t\tgot result for {call_contract:?}");
                    let return_data = return_data.data.clone().to_vec();
                    tracing::info!(
                        calldata = hex::encode(&call_input),
                        return_data = hex::encode(&return_data),
                        "mock call matched"
                    );
                    self.farcall_handler.set_immediate_return(return_data);
                    // return (return_data.ret_type, gas, return_data.data.clone());
                }
            }
            // let current = state.vm_local_state.callstack.current;
            // let calldata = get_calldata(&state, memory);
            // if let Some(return_data) = self.mocked_calls.get_matching_return_data(
            //     current.code_address,
            //     &calldata,
            //     U256::from(current.context_u128_value),
            // ) {
            //     tracing::info!(
            //         calldata = hex::encode(&calldata),
            //         return_data = hex::encode(&return_data),
            //         "mock call matched"
            //     );
            //     self.farcall_handler.set_immediate_return(return_data);
            // }
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
