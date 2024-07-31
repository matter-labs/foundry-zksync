use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use alloy_primitives::{hex, Address, Bytes, U256 as rU256};
use foundry_cheatcodes_common::{
    expect::ExpectedCallTracker,
    mock::{MockCallDataContext, MockCallReturnData},
    record::RecordAccess,
};
use multivm::{
    interface::{dyn_tracers::vm_1_5_0::DynTracer, tracer::TracerExecutionStatus},
    vm_latest::{BootloaderState, HistoryMode, SimpleMemory, VmTracer, ZkSyncVmState},
    zk_evm_latest::{
        tracing::{AfterDecodingData, AfterExecutionData, BeforeExecutionData, VmLocalStateData},
        zkevm_opcode_defs::{FatPointer, Opcode, CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER},
    },
};
use once_cell::sync::OnceCell;
use zksync_state::WriteStorage;
use zksync_types::{
    BOOTLOADER_ADDRESS, CONTRACT_DEPLOYER_ADDRESS, H256, SYSTEM_CONTEXT_ADDRESS, U256,
};

use crate::{
    convert::{ConvertAddress, ConvertH160, ConvertH256, ConvertU256},
    vm::farcall::{CallAction, CallDepth},
};

use super::farcall::FarCallHandler;

/// Selector for retrieving account version.
/// This is used to override the caller's account version when deploying a contract
/// So non-EOA addresses can also deploy within the VM.
///
/// extendedAccountVersion(address)
const SELECTOR_ACCOUNT_VERSION: [u8; 4] = hex!("bb0fd610");

/// Selector for  executing a VM transaction.
/// This is used to override the `msg.sender` for the call context
/// to account for transitive calls.
///
/// executeTransaction(bytes32, bytes32, tuple)
const SELECTOR_EXECUTE_TRANSACTION: [u8; 4] = hex!("df9c1589");

/// Selector for retrieving the current block number.
/// This is used to override the current `block.number` to foundry test's context.
///
/// Selector for `getBlockNumber()`
const SELECTOR_SYSTEM_CONTEXT_BLOCK_NUMBER: [u8; 4] = hex!("42cbb15c");

/// Selector for retrieving the current block timestamp.
/// This is used to override the current `block.timestamp` to foundry test's context.
///
/// Selector for `getBlockTimestamp()`
const SELECTOR_SYSTEM_CONTEXT_BLOCK_TIMESTAMP: [u8; 4] = hex!("796b89b9");

/// Selector for retrieving the current base fee.
/// This is used to override the current `block.basefee` to foundry test's context.
///
/// Selector for `baseFee()`
const SELECTOR_BASE_FEE: [u8; 4] = hex!("6ef25c3a");

/// Selector for retrieving the blockhash of a given block.
/// This is used to override the current `blockhash()` to foundry test's context.
///
/// Selector for `getBlockHashEVM(uint256)`
const SELECTOR_BLOCK_HASH: [u8; 4] = hex!("80b41246");

/// Represents the context for [CheatcodeContext]
#[derive(Debug, Default)]
pub struct CheatcodeTracerContext<'a> {
    /// Mocked calls.
    pub mocked_calls: HashMap<Address, BTreeMap<MockCallDataContext, MockCallReturnData>>,
    /// Expected calls recorder.
    pub expected_calls: Option<&'a mut ExpectedCallTracker>,
    /// Recorded storage accesses
    pub accesses: Option<&'a mut RecordAccess>,
    /// Factory deps that were persisted across calls
    pub persisted_factory_deps: Option<&'a mut HashMap<H256, Vec<u8>>>,
}

/// Tracer result to return back to foundry.
#[derive(Debug, Default)]
pub struct CheatcodeTracerResult {
    pub expected_calls: ExpectedCallTracker,
}

/// Defines the context for a Vm call.
#[derive(Debug, Default, Clone)]
pub struct CallContext {
    /// The transaction caller.
    pub tx_caller: Address,
    /// Value for `msg.sender`.
    pub msg_sender: Address,
    /// Target contract's address.
    pub contract: Address,
    /// Delegated contract's address. This is used
    /// to override `address(this)` for delegate calls.
    pub delegate_as: Option<Address>,
    /// The current block number
    pub block_number: rU256,
    /// The current block timestamp
    pub block_timestamp: rU256,
    /// The current block basefee
    pub block_basefee: rU256,
    /// Whether the current call is a create.
    pub is_create: bool,
    /// Whether the current call is a static call.
    pub is_static: bool,
    /// L1 block hashes to return when `BLOCKHASH` opcode is encountered. This ensures consistency
    /// when returning environment data in L2.
    pub block_hashes: HashMap<alloy_primitives::U256, alloy_primitives::FixedBytes<32>>,
}

/// A tracer to allow for foundry-specific functionality.
#[derive(Debug, Default)]
pub struct CheatcodeTracer {
    /// List of mocked calls.
    pub mocked_calls: HashMap<Address, BTreeMap<MockCallDataContext, MockCallReturnData>>,
    /// Tracked for foundry's expected calls.
    pub expected_calls: ExpectedCallTracker,
    /// Defines the current call context.
    pub call_context: CallContext,
    /// Result to send back.
    pub result: Arc<OnceCell<CheatcodeTracerResult>>,
    /// Handle farcall state.
    farcall_handler: FarCallHandler,
}

impl CheatcodeTracer {
    /// Create an instance of [CheatcodeTracer].
    pub fn new(
        mocked_calls: HashMap<Address, BTreeMap<MockCallDataContext, MockCallReturnData>>,
        expected_calls: ExpectedCallTracker,
        result: Arc<OnceCell<CheatcodeTracerResult>>,
        call_context: CallContext,
    ) -> Self {
        CheatcodeTracer { mocked_calls, expected_calls, call_context, result, ..Default::default() }
    }
}

impl<S, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for CheatcodeTracer {
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
        _state: VmLocalStateData<'_>,
        _data: BeforeExecutionData,
        _memory: &SimpleMemory<H>,
        _storage: zksync_state::StoragePtr<S>,
    ) {
    }

    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterExecutionData,
        memory: &SimpleMemory<H>,
        _storage: zksync_state::StoragePtr<S>,
    ) {
        self.farcall_handler.track_call_actions(&state, &data);

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

        // Handle mocked calls
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let current = state.vm_local_state.callstack.current;
            let call_input = get_calldata(&state, memory);
            let call_contract = current.code_address.to_address();
            let call_value = U256::from(current.context_u128_value).to_ru256();

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
                    tracing::info!(
                        "returning mocked value {:?} for {:?}",
                        hex::encode(&call_input),
                        hex::encode(&return_data)
                    );
                    self.farcall_handler.set_immediate_return(return_data);
                    return;
                }
            }
        }

        // Mark the caller as EOA to avoid panic. This is probably not needed anymore
        // since we manually override the ACCOUNT_CODE_STORAGE to return `0` for the caller.
        // TODO remove this and verify once we are stable.
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let current = state.vm_local_state.callstack.get_current_stack();
            let calldata = get_calldata(&state, memory);

            if current.code_address == CONTRACT_DEPLOYER_ADDRESS &&
                calldata.starts_with(&SELECTOR_ACCOUNT_VERSION)
            {
                let address = H256::from_slice(&calldata[4..36]).to_h160().to_address();
                if self.call_context.tx_caller == address {
                    tracing::debug!("overriding account version for caller {address:?}");
                    self.farcall_handler.set_immediate_return(rU256::from(1u32).to_be_bytes_vec());
                    return
                }
            }
        }

        // Override msg.sender for the transaction
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let calldata = get_calldata(&state, memory);
            let current = state.vm_local_state.callstack.current;

            if current.msg_sender == BOOTLOADER_ADDRESS &&
                calldata.starts_with(&SELECTOR_EXECUTE_TRANSACTION)
            {
                self.farcall_handler.set_action(
                    CallDepth::next(),
                    CallAction::SetMessageSender(self.call_context.msg_sender),
                );
            }
        }

        // Override block number and timestamp for the transaction
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let calldata = get_calldata(&state, memory);
            let current = state.vm_local_state.callstack.current;

            if current.code_address == SYSTEM_CONTEXT_ADDRESS {
                if calldata.starts_with(&SELECTOR_SYSTEM_CONTEXT_BLOCK_NUMBER) {
                    self.farcall_handler
                        .set_immediate_return(self.call_context.block_number.to_be_bytes_vec());
                    return
                } else if calldata.starts_with(&SELECTOR_SYSTEM_CONTEXT_BLOCK_TIMESTAMP) {
                    self.farcall_handler
                        .set_immediate_return(self.call_context.block_timestamp.to_be_bytes_vec());
                    return
                }
            }
        }

        // Override block base fee for the transaction. This is properly setup when creating
        // `L1BatchEnv` but a value of `0` is auto-translated to `1`, so we ensure that it will
        // always be `0`.
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let calldata = get_calldata(&state, memory);
            let current = state.vm_local_state.callstack.current;

            if current.code_address == SYSTEM_CONTEXT_ADDRESS &&
                calldata.starts_with(&SELECTOR_BASE_FEE)
            {
                self.farcall_handler
                    .set_immediate_return(self.call_context.block_basefee.to_be_bytes_vec());
                return
            }
        }

        // Override blockhash
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let calldata = get_calldata(&state, memory);
            let current = state.vm_local_state.callstack.current;

            if current.code_address == SYSTEM_CONTEXT_ADDRESS &&
                calldata.starts_with(&SELECTOR_BLOCK_HASH)
            {
                let block_number = U256::from(&calldata[4..36]);
                let block_hash = self
                    .call_context
                    .block_hashes
                    .get(&block_number.to_ru256())
                    .unwrap_or_default();
                self.farcall_handler.set_immediate_return(block_hash.to_vec());
                return;
            }
        }

        if let Some(delegate_as) = self.call_context.delegate_as {
            if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
                let current = state.vm_local_state.callstack.current;
                if current.code_address.to_address() == self.call_context.contract {
                    self.farcall_handler
                        .set_action(CallDepth::current(), CallAction::SetThisAddress(delegate_as));
                }
            }
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for CheatcodeTracer {
    fn initialize_tracer(&mut self, _state: &mut ZkSyncVmState<S, H>) {}

    fn finish_cycle(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        for action in self.farcall_handler.take_immediate_actions(state, bootloader_state) {
            match action {
                CallAction::SetMessageSender(sender) => {
                    tracing::info!(old=?state.local_state.callstack.current.msg_sender, new=?sender, "set msg.sender");
                    state.local_state.callstack.current.msg_sender = sender.to_h160();
                }
                CallAction::SetThisAddress(addr) => {
                    tracing::info!(old=?state.local_state.callstack.current.this_address, new=?addr, "set address(this)");
                    state.local_state.callstack.current.this_address = addr.to_h160();
                }
            }
        }
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
