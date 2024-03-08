use std::{collections::HashMap, fmt::Debug};

use foundry_common::conversion_utils::h256_to_h160;
use foundry_evm_core::{
    backend::DatabaseExt,
    era_revm::{db::RevmDatabaseForEra, storage_view::StorageView},
};
use itertools::Itertools;
use multivm::{
    vm_latest::{BootloaderState, HistoryMode, SimpleMemory, ZkSyncVmState},
    zk_evm_1_3_3::aux_structures::MemoryPage,
    zk_evm_1_4_0::{
        tracing::VmLocalStateData,
        vm_state::{self, PrimitiveValue},
        zkevm_opcode_defs::{
            decoding::{EncodingModeProduction, VmEncodingMode},
            FarCallABI, FatPointer, Opcode, CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER,
            CALL_SYSTEM_ABI_REGISTERS, RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER,
        },
    },
};
use zksync_basic_types::{H160, U256};
use zksync_state::StoragePtr;
use zksync_types::{Timestamp, MSG_VALUE_SIMULATOR_ADDRESS};
use zksync_utils::u256_to_h256;

type EraDb<DB> = StorageView<RevmDatabaseForEra<DB>>;
type PcOrImm = <EncodingModeProduction as VmEncodingMode<8>>::PcOrImm;
type CallStackEntry = vm_state::CallStackEntry<8, EncodingModeProduction>;

/// Contains information about the immediate return from a FarCall.
#[derive(Debug, Default, Clone)]
pub(crate) struct ImmediateReturn {
    pub(crate) return_data: Vec<u8>,
    pub(crate) continue_pc: PcOrImm,
    pub(crate) base_memory_page: u32,
    pub(crate) code_page: u32,
}

/// Tracks state of FarCalls to be able to return from them earlier.
/// This effectively short-circuits the execution and ignores following opcodes.
///
/// TODO: Add other FarCall functionality from the cheatcode implementation here.
#[derive(Debug, Default, Clone)]
pub(crate) struct FarCallHandler {
    pub(crate) active_far_call_stack: Option<CallStackEntry>,
    pub(crate) immediate_return: Option<ImmediateReturn>,
}

impl FarCallHandler {
    /// Marks the current FarCall opcode to return immediately during `finish_cycle`.
    /// Must be called during either `before_execution` or `after_execution`.
    pub(crate) fn set_immediate_return(&mut self, return_data: Vec<u8>) {
        if let Some(current) = &self.active_far_call_stack {
            self.immediate_return.replace(ImmediateReturn {
                return_data,
                continue_pc: current.pc.saturating_add(1),
                base_memory_page: current.base_memory_page.0,
                code_page: current.code_page.0,
            });
        } else {
            tracing::warn!("No active far call stack, ignoring immediate return");
        }
    }

    /// Tracks the call stack for the currently active FarCall.
    /// Must be called during `before_execution`.
    pub(crate) fn track_active_far_calls<S, H: HistoryMode>(
        &mut self,
        state: VmLocalStateData<'_>,
        data: multivm::zk_evm_1_4_0::tracing::BeforeExecutionData,
        _memory: &SimpleMemory<H>,
        _storage: StoragePtr<EraDb<S>>,
    ) {
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            self.active_far_call_stack.replace(state.vm_local_state.callstack.current);
        }
    }

    /// Attempts to return the preset data ignoring any following opcodes, if set.
    /// Must be called during `finish_cycle`.
    pub(crate) fn maybe_return_early<S: DatabaseExt + Send, H: HistoryMode>(
        &mut self,
        state: &mut ZkSyncVmState<EraDb<S>, H>,
        _bootloader_state: &mut BootloaderState,
        _storage: StoragePtr<EraDb<S>>,
    ) {
        if let Some(immediate_return) = self.immediate_return.take() {
            // set return data
            let data_chunks = immediate_return.return_data.chunks(32);
            let return_memory_page =
                CallStackEntry::heap_page_from_base(MemoryPage(immediate_return.base_memory_page));
            let return_fat_ptr = FatPointer {
                memory_page: return_memory_page.0,
                offset: 0,
                start: 0,
                length: (data_chunks.len() as u32) * 32,
            };
            let start_slot = (return_fat_ptr.start / 32) as usize;
            let data = data_chunks
                .enumerate()
                .map(|(index, value)| (start_slot + index, U256::from_big_endian(value)))
                .collect_vec();
            state.local_state.registers[RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER as usize] =
                PrimitiveValue { value: return_fat_ptr.to_u256(), is_pointer: true };
            state.memory.populate_page(
                return_fat_ptr.memory_page as usize,
                data,
                Timestamp(state.local_state.timestamp),
            );

            // change current stack to simulate return
            let current = state.local_state.callstack.get_current_stack_mut();
            current.pc = immediate_return.continue_pc;
            current.base_memory_page = MemoryPage(immediate_return.base_memory_page);
            current.code_page = MemoryPage(immediate_return.code_page);
        }
    }
}

/// Defines the [MockCall]s return type.
type MockCallReturn = Vec<u8>;

/// Defines the match criteria of a mocked call.
#[derive(Default, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MockCall {
    pub address: H160,
    pub value: Option<U256>,
    pub calldata: Vec<u8>,
}

/// Contains the list of mocked calls.
/// Note that mocked calls with value take precedence of the ones without.
#[derive(Default, Debug, Clone)]
pub struct MockedCalls {
    /// List of mocked calls with the value parameter.
    with_value: HashMap<MockCall, MockCallReturn>,

    /// List of mocked calls without the value parameter.
    without_value: HashMap<MockCall, MockCallReturn>,
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
// Selector for `ContractDeployer::create(bytes32, bytes32, bytes)`
pub const SELECTOR_CONTRACT_DEPLOYER_CREATE: &str = "9c4d535b";
// Selector for `ContractDeployer::create2(bytes32, bytes32, bytes)`
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
    pub fn to(&self) -> &H160 {
        match self {
            ParsedFarCall::ValueCall { to, .. } => to,
            ParsedFarCall::SimpleCall { to, .. } => to,
        }
    }

    /// Retrieves the `value` for the call
    pub fn value(&self) -> &U256 {
        match self {
            ParsedFarCall::ValueCall { value, .. } => value,
            ParsedFarCall::SimpleCall { value, .. } => value,
        }
    }

    /// Retrieves the selector for the call, or returns an empty string if none.
    pub fn selector(&self) -> String {
        let calldata = self.calldata();

        if calldata.len() < 4 {
            String::from("")
        } else {
            hex::encode(&calldata[0..4])
        }
    }

    /// Retrieves the calldata for the call, if any
    pub fn calldata(&self) -> &[u8] {
        match self {
            ParsedFarCall::ValueCall { calldata, .. } => calldata,
            ParsedFarCall::SimpleCall { calldata, .. } => calldata,
        }
    }

    /// Retrieves the parameters from calldata, if any
    pub fn params(&self) -> Vec<[u8; 32]> {
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
    pub fn param_bytes_after(&self, offset_words: usize) -> Vec<u8> {
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
pub fn parse<H: HistoryMode>(
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
        let address = u256_to_h256(reg[MSG_VALUE_SIMULATOR_DATA_ADDRESS_REG as usize].value);
        let address = h256_to_h160(&address);
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
