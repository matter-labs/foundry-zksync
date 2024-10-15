#![allow(unused)]

use std::{collections::HashMap, default, fmt::Debug};

use alloy_primitives::{hex, Address};
use itertools::Itertools;
use zksync_basic_types::{H160, U256};
use zksync_multivm::{
    vm_1_3_2::zk_evm_1_3_3::zkevm_opcode_defs::RetABI,
    vm_latest::{BootloaderState, HistoryMode, SimpleMemory, ZkSyncVmState},
    zk_evm_latest::{
        aux_structures::{MemoryPage, Timestamp},
        opcodes::DecodedOpcode as ZkDecodedOpcode,
        tracing::{AfterExecutionData, BeforeExecutionData, VmLocalStateData},
        vm_state::{self, code_page_candidate_from_base, PrimitiveValue},
        zkevm_opcode_defs::{
            decoding::{EncodingModeProduction, VmEncodingMode},
            Condition, DecodedOpcode as ZkDecodedOpcodeDef, FarCallABI, FarCallOpcode, FatPointer,
            Opcode, OpcodeVariant, Operand, RetOpcode, CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER,
            CALL_SYSTEM_ABI_REGISTERS, RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER,
        },
    },
};
use zksync_state::interface::{StoragePtr, WriteStorage};
use zksync_types::MSG_VALUE_SIMULATOR_ADDRESS;

use crate::convert::{ConvertAddress, ConvertH256, ConvertU256};

type PcOrImm = <EncodingModeProduction as VmEncodingMode<8>>::PcOrImm;
type CallStackEntry = vm_state::CallStackEntry<8, EncodingModeProduction>;
type DecodedOpcode = ZkDecodedOpcode<8, EncodingModeProduction>;
type DecodedOpcodeDef = ZkDecodedOpcodeDef<8, EncodingModeProduction>;

/// The call depth
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct CallDepth(u8);

impl CallDepth {
    /// Create a new [CallDepth] instance.
    #[inline]
    pub(crate) const fn new(depth: u8) -> CallDepth {
        CallDepth(depth)
    }

    /// Create a [CallDepth] with depth `0`.
    #[inline]
    pub(crate) const fn current() -> CallDepth {
        CallDepth(0)
    }

    /// Create a [CallDepth] with depth `1`.
    #[inline]
    pub(crate) const fn next() -> CallDepth {
        CallDepth(1)
    }

    /// Decrement [CallDepth] until the value of `0`.
    #[inline]
    pub(crate) fn decrement(self) -> CallDepth {
        CallDepth(self.0.saturating_sub(1))
    }
}

/// The call action.
#[derive(Debug, Clone)]
pub(crate) enum CallAction {
    /// Assign msg.sender.
    SetMessageSender(Address),
    /// Assign address(this).
    SetThisAddress(Address),
}

/// The call action.
#[derive(Debug, Default, Clone)]
pub(crate) struct CallActions {
    // The [CallAction]s for the current call depth of `0`.
    // These are immediately executed in the current `finish_cycle`.
    immediate: Vec<CallAction>,

    /// The specified [CallAction]s for the next `CallDepth`.
    /// A depth of `0` indicates an immediate action, and as such the action
    /// will be moved to `[CallActions::immediate] on the next call to [CallActions::track].
    pending: Vec<(CallDepth, CallAction)>,
}

impl CallActions {
    /// Insert a call action.
    pub(crate) fn push(&mut self, depth: CallDepth, action: CallAction) {
        if depth == CallDepth::current() {
            self.immediate.push(action);
        } else {
            self.pending.push((depth.decrement(), action));
        }
    }

    /// Track pending [CallAction]s, decrementing the depth if it's not ready.
    pub(crate) fn track(&mut self) {
        let mut pending_actions = vec![];
        for (depth, action) in self.pending.iter().cloned() {
            if depth == CallDepth::current() {
                self.immediate.push(action);
            } else {
                pending_actions.push((depth.decrement(), action));
            }
        }
        self.pending = pending_actions;
    }

    /// Consume the immediate actions.
    pub(crate) fn take_immediate(&mut self) -> Vec<CallAction> {
        std::mem::take(&mut self.immediate)
    }
}

/// Tracks state of FarCalls to be able to return from them earlier.
/// This effectively short-circuits the execution and ignores following opcodes.
#[derive(Debug, Default, Clone)]
pub(crate) struct FarCallHandler {
    pub(crate) before_far_call_stack: Option<CallStackEntry>,
    pub(crate) after_far_call_stack: Option<CallStackEntry>,
    pub(crate) current_far_call: Option<FarCallOpcode>,
    pub(crate) immediate_return: Option<Vec<u8>>,
    call_actions: CallActions,
}

impl FarCallHandler {
    /// Marks the current FarCall opcode to return immediately during `finish_cycle`.
    /// Must be called during either `before_execution` or `after_execution`.
    /// Must _always_ be called always within a `Farcall` opcode.
    pub(crate) fn set_immediate_return(&mut self, return_data: Vec<u8>) {
        self.immediate_return.replace(return_data);
    }

    /// Sets a [CallAction] for the current or subsequent FarCalls during `finish_cycle`.
    /// Must be called during either `before_execution` or `after_execution`.
    pub(crate) fn set_action(&mut self, depth: CallDepth, action: CallAction) {
        self.call_actions.push(depth, action)
    }

    /// Tracks the call stack for the currently executable [CallAction]s.
    /// Must be called during `after_execution`.
    pub(crate) fn track_call_actions(
        &mut self,
        state: &VmLocalStateData<'_>,
        data: &AfterExecutionData,
    ) {
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            self.call_actions.track();
        }
    }

    /// Attempts to return the preset data ignoring any following opcodes, if set.
    /// Must be called during `finish_cycle`.
    pub(crate) fn maybe_return_early<S: WriteStorage, H: HistoryMode>(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &mut BootloaderState,
    ) {
        if let Some(return_data) = self.immediate_return.take() {
            // set return data
            let current = *state.local_state.callstack.get_current_stack();
            let return_memory_page = vm_state::heap_page_from_base(current.base_memory_page);
            let return_data_len = return_data.len();
            populate_page_with_data(state, return_memory_page, return_data);

            // memory_page of 0 is fine since heap page will be assigned automatically
            let return_fat_ptr =
                FatPointer { memory_page: 0, offset: 0, start: 0, length: return_data_len as u32 };
            // Note that register `0` should contain `RetABI`. However besides the pointer the other
            // fields can be zero for our purposes.
            // `is_pointer` must be false as it is not a pointer, but rather a heap slice
            state.local_state.registers[0] =
                PrimitiveValue { value: return_fat_ptr.to_u256(), is_pointer: false };

            // Just rewriting `code_page` is very error-prone, since the same memory page would be
            // re-used for decommitments. We'll use a different approach:
            // - Set `previous_code_word` to the value that we want
            // - Set `previous_code_memory_page` to the current code page + `previous_super_pc` to 0
            //   (as it corresponds
            // to the current pc of 0). This will ensure that VM will *not even read* from the code
            // memory page.
            let immediate_return_opcode = DecodedOpcode {
                inner: DecodedOpcodeDef {
                    variant: OpcodeVariant {
                        opcode: Opcode::Ret(RetOpcode::Ok),
                        src0_operand_type: Operand::RegOnly,
                        dst0_operand_type: Operand::RegOnly,
                        flags: [false; 2],
                    },
                    condition: Condition::Always,
                    // src0 is the only thing that matters.
                    // it means that the ret abi will be taken from r1
                    src0_reg_idx: 1,
                    src1_reg_idx: 0,
                    dst0_reg_idx: 0,
                    dst1_reg_idx: 0,
                    imm_0: 0,
                    imm_1: 0,
                },
            };
            state.local_state.previous_code_word =
                U256([0, 0, 0, immediate_return_opcode.serialize_as_integer()]);
            state.local_state.previous_code_memory_page =
                state.local_state.callstack.current.code_page;
            state.local_state.previous_super_pc = 0;

            // Just in case to avoid any gas costs related to memory
            state.local_state.callstack.current.heap_bound = u32::MAX;
        }
    }

    /// Returns immediate [CallAction]s for the currently active FarCall.
    /// Must be called during `finish_cycle`.
    pub(crate) fn take_immediate_actions<S: WriteStorage, H: HistoryMode>(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &mut BootloaderState,
    ) -> Vec<CallAction> {
        self.call_actions.take_immediate()
    }
}

fn populate_page_with_data<S: WriteStorage, H: HistoryMode>(
    state: &mut ZkSyncVmState<S, H>,
    page: MemoryPage,
    data: Vec<u8>,
) {
    let data_chunks = data.chunks(32);

    let start_slot = 0;
    let data = data_chunks
        .enumerate()
        .map(|(index, value)| (start_slot + index, U256::from_big_endian(value)))
        .collect_vec();

    state.memory.populate_page(page.0 as usize, data, Timestamp(state.local_state.timestamp));
}

/// Defines the [MockCall]s return type.
type MockCallReturn = Vec<u8>;

/// Defines the match criteria of a mocked call.
#[derive(Default, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MockCall {
    pub(crate) address: H160,
    pub(crate) value: Option<U256>,
    pub(crate) calldata: Vec<u8>,
}

/// Contains the list of mocked calls.
/// Note that mocked calls with value take precedence of the ones without.
#[derive(Default, Debug, Clone)]
pub(crate) struct MockedCalls {
    /// List of mocked calls with the value parameter.
    pub(crate) with_value: HashMap<MockCall, MockCallReturn>,

    /// List of mocked calls without the value parameter.
    pub(crate) without_value: HashMap<MockCall, MockCallReturn>,
}

impl MockedCalls {
    /// Insert a mocked call with its return data.
    pub(crate) fn insert(&mut self, call: MockCall, return_data: MockCallReturn) {
        if call.value.is_some() {
            self.with_value.insert(call, return_data);
        } else {
            self.without_value.insert(call, return_data);
        }
    }

    /// Clear all mocked calls.
    pub(crate) fn clear(&mut self) {
        self.with_value.clear();
        self.without_value.clear();
    }

    /// Matches the mocked calls based on foundry rules. The matching is in the precedence order of:
    /// * Calls with value parameter and exact calldata match
    /// * Exact calldata matches
    /// * Partial calldata matches
    pub(crate) fn get_matching_return_data(
        &self,
        code_address: H160,
        actual_calldata: &[u8],
        actual_value: U256,
    ) -> Option<Vec<u8>> {
        let mut best_match = None;

        for (call, call_return_data) in self.with_value.iter().chain(self.without_value.iter()) {
            if call.address == code_address {
                let value_matches = call.value.map_or(true, |value| value == actual_value);
                if !value_matches {
                    continue
                }

                if actual_calldata.starts_with(&call.calldata) {
                    // return early if exact match
                    if call.calldata.len() == actual_calldata.len() {
                        return Some(call_return_data.clone())
                    }

                    // else check for partial matches and pick the best
                    let matched_len = call.calldata.len();
                    best_match = best_match.map_or(
                        Some((matched_len, call_return_data)),
                        |(best_match, best_match_return_data)| {
                            if matched_len > best_match {
                                Some((matched_len, call_return_data))
                            } else {
                                Some((best_match, best_match_return_data))
                            }
                        },
                    );
                }
            }
        }

        best_match.map(|(_, return_data)| return_data.clone())
    }
}

/// Selector for `L2EthToken::balanceOf(uint256)`
pub const SELECTOR_L2_ETH_BALANCE_OF: &str = "9cc7f708";
/// Selector for `SystemContext::getBlockNumber()`
pub const SELECTOR_SYSTEM_CONTEXT_BLOCK_NUMBER: &str = "42cbb15c";
/// Selector for `SystemContext::getBlockTimestamp()`
pub const SELECTOR_SYSTEM_CONTEXT_BLOCK_TIMESTAMP: &str = "796b89b9";
/// Selector for `ContractDeployer::create(bytes32, bytes32, bytes)`
pub const SELECTOR_CONTRACT_DEPLOYER_CREATE: &str = "9c4d535b";
/// Selector for `ContractDeployer::create2(bytes32, bytes32, bytes)`
pub const SELECTOR_CONTRACT_DEPLOYER_CREATE2: &str = "3cda3351";

/// Represents a parsed FarCall from the ZK-EVM
pub enum ParsedFarCall {
    /// A call to MsgValueSimulator contract used when transferring ETH
    ValueCall { to: H160, value: U256, calldata: Vec<u8>, recipient: H160, is_system_call: bool },
    /// A simple FarCall with calldata.
    SimpleCall { to: H160, value: U256, calldata: Vec<u8> },
}

impl ParsedFarCall {
    /// Retrieves the `to` address for the call, if any
    pub(crate) fn to(&self) -> &H160 {
        match self {
            ParsedFarCall::ValueCall { to, .. } => to,
            ParsedFarCall::SimpleCall { to, .. } => to,
        }
    }

    /// Retrieves the `value` for the call
    pub(crate) fn value(&self) -> &U256 {
        match self {
            ParsedFarCall::ValueCall { value, .. } => value,
            ParsedFarCall::SimpleCall { value, .. } => value,
        }
    }

    /// Retrieves the selector for the call, or returns an empty string if none.
    pub(crate) fn selector(&self) -> String {
        let calldata = self.calldata();

        if calldata.len() < 4 {
            String::from("")
        } else {
            hex::encode(&calldata[0..4])
        }
    }

    /// Retrieves the calldata for the call, if any
    pub(crate) fn calldata(&self) -> &[u8] {
        match self {
            ParsedFarCall::ValueCall { calldata, .. } => calldata,
            ParsedFarCall::SimpleCall { calldata, .. } => calldata,
        }
    }

    /// Retrieves the parameters from calldata, if any
    pub(crate) fn params(&self) -> Vec<[u8; 32]> {
        let params = &match self {
            ParsedFarCall::ValueCall { calldata, .. } => calldata,
            ParsedFarCall::SimpleCall { calldata, .. } => calldata,
        }[4..];
        if params.is_empty() {
            return Vec::new()
        }

        params
            .chunks(32)
            .map(|c| c.try_into().expect("chunk must be exactly 32 bytes"))
            .collect_vec()
    }

    /// Retrieves all bytes after the `offset` number of 32byte words
    pub(crate) fn param_bytes_after(&self, offset_words: usize) -> Vec<u8> {
        let params = &match self {
            ParsedFarCall::ValueCall { calldata, .. } => calldata,
            ParsedFarCall::SimpleCall { calldata, .. } => calldata,
        }[4..];
        if params.is_empty() || params.len() < 32 * offset_words {
            return Vec::new()
        }

        params[32 * offset_words..].to_vec()
    }
}

impl Debug for ParsedFarCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParsedFarCall::ValueCall { to, value, calldata, recipient, is_system_call } => f
                .debug_struct("ValueCall")
                .field("to", to)
                .field("value", value)
                .field("calldata", &hex::encode(calldata))
                .field("recipient", recipient)
                .field("is_system_call", is_system_call)
                .finish(),
            ParsedFarCall::SimpleCall { to, value, calldata } => f
                .debug_struct("SimpleCall")
                .field("to", to)
                .field("value", value)
                .field("calldata", &hex::encode(calldata))
                .finish(),
        }
    }
}

const MSG_VALUE_SIMULATOR_ADDRESS_EXTRA_PARAM_REG_OFFSET: u8 = CALL_SYSTEM_ABI_REGISTERS.start;
const MSG_VALUE_SIMULATOR_DATA_VALUE_REG: u8 = MSG_VALUE_SIMULATOR_ADDRESS_EXTRA_PARAM_REG_OFFSET;
const MSG_VALUE_SIMULATOR_DATA_ADDRESS_REG: u8 =
    MSG_VALUE_SIMULATOR_ADDRESS_EXTRA_PARAM_REG_OFFSET + 1;
const MSG_VALUE_SIMULATOR_DATA_IS_SYSTEM_REG: u8 =
    MSG_VALUE_SIMULATOR_ADDRESS_EXTRA_PARAM_REG_OFFSET + 2;
const MSG_VALUE_SIMULATOR_IS_SYSTEM_BIT: u8 = 1;

/// Parses a FarCall into ZKSync's normal calls or MsgValue calls.
/// For MsgValueSimulator call parsing, see https://github.com/matter-labs/era-system-contracts/blob/main/contracts/MsgValueSimulator.sol#L25
/// For normal call parsing, see https://github.com/matter-labs/zksync-era/blob/main/core/lib/multivm/src/tracers/call_tracer/vm_latest/mod.rs#L115
pub(crate) fn parse<H: HistoryMode>(
    state: &VmLocalStateData<'_>,
    memory: &SimpleMemory<H>,
) -> ParsedFarCall {
    let current = state.vm_local_state.callstack.get_current_stack();
    let reg = &state.vm_local_state.registers;
    let value = U256::from(current.context_u128_value);

    let packed_abi = reg[CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER as usize];
    assert!(packed_abi.is_pointer);
    let far_call_abi = FarCallABI::from_u256(packed_abi.value);
    let calldata = memory.read_unaligned_bytes(
        far_call_abi.memory_quasi_fat_pointer.memory_page as usize,
        far_call_abi.memory_quasi_fat_pointer.start as usize,
        far_call_abi.memory_quasi_fat_pointer.length as usize,
    );
    if current.code_address == MSG_VALUE_SIMULATOR_ADDRESS {
        let value = U256::from(reg[MSG_VALUE_SIMULATOR_DATA_VALUE_REG as usize].value.low_u128());
        let address = reg[MSG_VALUE_SIMULATOR_DATA_ADDRESS_REG as usize].value.to_h256();
        let address = address.to_h160();
        let is_system_call = reg[MSG_VALUE_SIMULATOR_DATA_IS_SYSTEM_REG as usize]
            .value
            .bit(MSG_VALUE_SIMULATOR_IS_SYSTEM_BIT as usize);

        ParsedFarCall::ValueCall {
            to: current.code_address,
            value,
            calldata,
            recipient: address,
            is_system_call,
        }
    } else {
        ParsedFarCall::SimpleCall { to: current.code_address, value, calldata }
    }
}
