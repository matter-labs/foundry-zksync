use std::{
    cell::OnceCell,
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

use alloy_primitives::{hex, map::HashMap, Address, Bytes, FixedBytes, U256 as rU256};
use foundry_cheatcodes_common::{
    expect::ExpectedCallTracker,
    mock::{MockCallDataContext, MockCallReturnData},
    record::RecordAccess,
};
use tracing::debug;
use zksync_multivm::{
    interface::tracer::TracerExecutionStatus,
    tracers::dynamic::vm_1_5_0::DynTracer,
    vm_latest::{BootloaderState, HistoryMode, SimpleMemory, VmTracer, ZkSyncVmState},
    zk_evm_latest::{
        tracing::{AfterDecodingData, AfterExecutionData, BeforeExecutionData, VmLocalStateData},
        zkevm_opcode_defs::{
            FarCallOpcode, FatPointer, Opcode, CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER,
        },
    },
};
use zksync_types::{
    ethabi, get_code_key, get_nonce_key, StorageValue, BOOTLOADER_ADDRESS,
    CONTRACT_DEPLOYER_ADDRESS, H160, H256, IMMUTABLE_SIMULATOR_STORAGE_ADDRESS,
    SYSTEM_CONTEXT_ADDRESS, U256,
};
use zksync_vm_interface::storage::{ReadStorage, StoragePtr, WriteStorage};

use crate::{
    compute_create2_address, compute_create_address,
    convert::{ConvertAddress, ConvertH160, ConvertH256, ConvertU256},
    hash_bytecode,
    state::{parse_full_nonce, FullNonce},
    vm::{
        farcall::{CallAction, CallDepth, CallExecutionStatus, FarCallHandler, TxExecutionStatus},
        storage_recorder::CallType,
        storage_view::StorageViewRecorder,
        ZkEnv, HARDHAT_CONSOLE_ADDRESS,
    },
    ZkPaymasterData, EMPTY_CODE,
};

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
pub(crate) const SELECTOR_EXECUTE_TRANSACTION: [u8; 4] = hex!("df9c1589");

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

/// Selector for setting immutables for an address.
/// This is used to retrieve the immutables and use them in merging storage
/// during forks.
///
/// Selector for `setImmutables(address, (uint256,bytes32)[])",
const SELECTOR_IMMUTABLE_SIMULATOR_SET: [u8; 4] = hex!("ad7e232e");

/// create(bytes32, bytes32, bytes)
const SELECTOR_CONTRACT_DEPLOYER_CREATE: [u8; 4] = hex!("9c4d535b");
/// create2(bytes32, bytes32, bytes)
const SELECTOR_CONTRACT_DEPLOYER_CREATE2: [u8; 4] = hex!("3cda3351");

/// Represents the context for [CheatcodeContext]
#[derive(Debug, Default)]
pub struct CheatcodeTracerContext<'a> {
    /// Mocked calls.
    pub mocked_calls: HashMap<Address, BTreeMap<MockCallDataContext, VecDeque<MockCallReturnData>>>,
    /// Expected calls recorder.
    pub expected_calls: Option<&'a mut ExpectedCallTracker>,
    /// Recorded storage accesses
    pub accesses: Option<&'a mut RecordAccess>,
    /// Factory deps that were persisted across calls
    pub persisted_factory_deps: Option<&'a mut HashMap<H256, Vec<u8>>>,
    /// Paymaster data
    pub paymaster_data: Option<ZkPaymasterData>,
    /// Era Vm environment
    pub zk_env: ZkEnv,
    /// Whether to record storage accesses.
    pub record_storage_accesses: bool,
}

/// Tracer result to return back to foundry.
#[derive(Debug, Default)]
pub struct CheatcodeTracerResult {
    /// Recorded expected calls.
    pub expected_calls: ExpectedCallTracker,
    /// Immutables recorded via calls to ImmutableSimulator::setImmutables.
    pub recorded_immutables: HashMap<H160, HashMap<rU256, FixedBytes<32>>>,
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
    /// Target contract's input (if CALL).
    pub input: Option<Bytes>,
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
    pub block_hashes: HashMap<rU256, FixedBytes<32>>,
    /// Whether to record storage accesses.
    pub record_storage_accesses: bool,
}

/// A tracer to allow for foundry-specific functionality.
#[derive(Debug, Default)]
pub struct CheatcodeTracer {
    /// List of mocked calls.
    pub mocked_calls: HashMap<Address, BTreeMap<MockCallDataContext, VecDeque<MockCallReturnData>>>,
    /// Tracked for foundry's expected calls.
    pub expected_calls: ExpectedCallTracker,
    /// Defines the current call context.
    pub call_context: CallContext,
    /// Result to send back.
    pub result: Arc<OnceCell<CheatcodeTracerResult>>,
    /// Handle farcall state.
    farcall_handler: FarCallHandler,
    /// Immutables recorded via calls to ImmutableSimulator::setImmutables.
    recorded_immutables: HashMap<H160, HashMap<rU256, FixedBytes<32>>>,
}

impl CheatcodeTracer {
    /// Create an instance of [CheatcodeTracer].
    pub fn new(
        mocked_calls: HashMap<Address, BTreeMap<MockCallDataContext, VecDeque<MockCallReturnData>>>,
        expected_calls: ExpectedCallTracker,
        result: Arc<OnceCell<CheatcodeTracerResult>>,
        call_context: CallContext,
    ) -> Self {
        CheatcodeTracer { mocked_calls, expected_calls, call_context, result, ..Default::default() }
    }

    /// Check if the given address's code is empty
    fn has_empty_code<S: ReadStorage>(
        &self,
        storage: &StoragePtr<S>,
        target: Address,
        calldata: &[u8],
        value: rU256,
    ) -> bool {
        // The following addresses are expected to have empty bytecode
        let ignored_known_addresses =
            [HARDHAT_CONSOLE_ADDRESS, self.call_context.tx_caller, self.call_context.msg_sender];

        // Skip empty code check for empty calldata with non-zero value (Transfers)
        if calldata.is_empty() && !value.is_zero() {
            return false;
        }

        let contract_code = storage.borrow_mut().read_value(&get_code_key(&target.to_h160()));

        !ignored_known_addresses.contains(&target) &&
            (contract_code == hash_bytecode(&EMPTY_CODE) ||
                contract_code == StorageValue::zero())
    }
}

impl<S: ReadStorage + StorageViewRecorder, H: HistoryMode> DynTracer<S, SimpleMemory<H>>
    for CheatcodeTracer
{
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
        _storage: StoragePtr<S>,
    ) {
    }

    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterExecutionData,
        memory: &SimpleMemory<H>,
        storage: StoragePtr<S>,
    ) {
        self.farcall_handler.track_call_actions(&state, &data);

        // We try to read calldata as late as possible since it's a very
        // slow operation. When we do read it, we do it only once per call
        // thru this cell
        let calldata = std::cell::LazyCell::new(|| get_calldata(&state, memory));
        let tx_tracking = self.farcall_handler.track_tx_execution(&state, &data, &calldata);

        if self.call_context.record_storage_accesses {
            // Record account accesses
            if tx_tracking.status_changed {
                match tx_tracking.status {
                    TxExecutionStatus::Pending => (),
                    TxExecutionStatus::Executing => {
                        storage.borrow_mut().start_recording();
                    }
                    TxExecutionStatus::Finished => {
                        if let Some(CallExecutionStatus::CallFinished(_tx)) =
                            &tx_tracking.call_status
                        {
                            storage.borrow_mut().record_call_end()
                        }
                        storage.borrow_mut().stop_recording()
                    }
                }
            }

            // record accesses
            if self.farcall_handler.is_tx_executing() {
                if let Some(call_status) = tx_tracking.call_status {
                    let current = state.vm_local_state.callstack.current;
                    // Check if we have the msg.sender override correction scheduled and
                    // account for it as it is not yet applied
                    // to some of the calls we record.
                    let msg_sender = self
                        .farcall_handler
                        .immediate_actions()
                        .iter()
                        .find_map(|action| match action {
                            CallAction::SetMessageSender(address) => Some(*address),
                            _ => None,
                        })
                        .unwrap_or_else(|| current.msg_sender.to_address());

                    let value = U256::from(current.context_u128_value);
                    let to = current.code_address;

                    let (call_type, account, data) = if to == CONTRACT_DEPLOYER_ADDRESS &&
                        (calldata.starts_with(&SELECTOR_CONTRACT_DEPLOYER_CREATE) ||
                            calldata.starts_with(&SELECTOR_CONTRACT_DEPLOYER_CREATE2))
                    {
                        let mut params = ethabi::decode(
                            &[
                                ethabi::ParamType::Uint(256),
                                ethabi::ParamType::Uint(256),
                                ethabi::ParamType::Bytes,
                            ],
                            &calldata[4..],
                        )
                        .expect("failed to decode transfer parameters");

                        let salt =
                            params.remove(0).into_uint().expect("must be valid uint256").to_h256();
                        let bytecode_hash =
                            params.remove(0).into_uint().expect("must be valid uint256").to_h256();
                        let constructor_input =
                            params.remove(0).into_bytes().expect("must be valid uint256");

                        let address = if calldata.starts_with(&SELECTOR_CONTRACT_DEPLOYER_CREATE) {
                            let full_nonce = storage
                                .borrow_mut()
                                .read_value(&get_nonce_key(&msg_sender.to_h160()));
                            let FullNonce { deploy_nonce, .. } =
                                parse_full_nonce(full_nonce.to_ru256());
                            compute_create_address(msg_sender, deploy_nonce as u32)
                        } else if calldata.starts_with(&SELECTOR_CONTRACT_DEPLOYER_CREATE2) {
                            compute_create2_address(
                                msg_sender,
                                bytecode_hash,
                                salt,
                                &constructor_input,
                            )
                        } else {
                            unreachable!()
                        };

                        (CallType::Create(bytecode_hash), address, constructor_input.to_vec())
                    } else {
                        (CallType::Call, to.to_address(), calldata.clone())
                    };

                    match call_status {
                        CallExecutionStatus::CallStart(tx) => {
                            storage.borrow_mut().record_call_start(
                                matches!(tx.opcode, FarCallOpcode::Mimic),
                                call_type,
                                msg_sender,
                                account,
                                data,
                                value.to_ru256(),
                            )
                        }
                        CallExecutionStatus::CallFinished(_tx) => {
                            storage.borrow_mut().record_call_end()
                        }
                    }
                }
            }
        }

        // Checks contract calls for expectCall cheatcode
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let current = state.vm_local_state.callstack.current;
            if let Some(expected_calls_for_target) =
                self.expected_calls.get_mut(&current.code_address.to_address())
            {
                // We skip recording the base call for `expectCall` cheatcode that initiated this
                // transaction. The initial call is recorded in revm when it was
                // made, and before being dispatched to zkEVM.
                let is_base_call = current.code_address.to_address() == self.call_context.contract &&
                    self.call_context
                        .input
                        .as_ref()
                        .map(|input| input.0.as_ref() == calldata.as_slice())
                        .unwrap_or_default();

                if !is_base_call {
                    // Match every partial/full calldata
                    for (expected_calldata, (expected, actual_count)) in expected_calls_for_target {
                        // Increment actual times seen if...
                        // The calldata is at most, as big as this call's input, and
                        if expected_calldata.len() <= calldata.len() &&
                        // Both calldata match, taking the length of the assumed smaller one (which will have at least the selector), and
                        *expected_calldata == calldata[..expected_calldata.len()] &&
                        // The value matches, if provided
                        expected.value.is_none_or(|value|{value == rU256::from(current.context_u128_value)})
                        {
                            *actual_count += 1;
                        }
                    }
                } else {
                    debug!("skip recording base call in zkEVM for expectCall cheatcode");
                }
            }
        }

        // Handle mocked calls
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let current = state.vm_local_state.callstack.current;
            let call_contract = current.code_address.to_address();
            let call_value = U256::from(current.context_u128_value).to_ru256();

            let mut had_mocks = false;
            if let Some(mocks) = self.mocked_calls.get_mut(&call_contract) {
                had_mocks = true;
                let ctx = MockCallDataContext {
                    calldata: Bytes::copy_from_slice(&calldata),
                    value: Some(call_value),
                };
                if let Some(return_data_queue) = match mocks.get_mut(&ctx) {
                    Some(queue) => Some(queue),
                    None => mocks
                        .iter_mut()
                        .find(|(mock, _)| {
                            calldata.get(..mock.calldata.len()) == Some(&mock.calldata[..]) &&
                                mock.value.is_none_or(|value| value == call_value)
                        })
                        .map(|(_, v)| v),
                } {
                    if let Some(return_data) = if return_data_queue.len() == 1 {
                        // If the mocked calls stack has a single element in it, don't empty it
                        return_data_queue.front().map(|x| x.to_owned())
                    } else {
                        // Else, we pop the front element
                        return_data_queue.pop_front()
                    } {
                        let return_data = return_data.data.clone().to_vec();
                        tracing::info!(
                            "returning mocked value {:?} for {:?}",
                            hex::encode(calldata.as_slice()),
                            hex::encode(&return_data)
                        );
                        self.farcall_handler.set_immediate_return(return_data);
                        return;
                    }
                }
            }

            // if we get here there was no matching mock call,
            // so we check if there's no code at the mocked address
            if self.has_empty_code(&storage, call_contract, calldata.as_slice(), call_value) {
                // issue a more targeted
                // error if we already had some mocks there
                let had_mocks_message =
                    if had_mocks { " - please ensure the current calldata is mocked" } else { "" };

                tracing::error!(
                    target = ?call_contract,
                    calldata = hex::encode(calldata.as_slice()),
                    "call may fail or behave unexpectedly due to empty code{}",
                    had_mocks_message
                );
            }
        }

        // Mark the caller as EOA to avoid panic. This is probably not needed anymore
        // since we manually override the ACCOUNT_CODE_STORAGE to return `0` for the caller.
        // TODO remove this and verify once we are stable.
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let current = state.vm_local_state.callstack.get_current_stack();

            if current.code_address == CONTRACT_DEPLOYER_ADDRESS &&
                calldata.starts_with(&SELECTOR_ACCOUNT_VERSION)
            {
                let address = H256::from_slice(&calldata[4..36]).to_h160().to_address();
                if self.call_context.tx_caller == address {
                    tracing::debug!("overriding account version for caller {address:?}");
                    self.farcall_handler.set_immediate_return(rU256::from(1u32).to_be_bytes_vec());
                    return;
                }
            }
        }

        // Override msg.sender for the execute transaction.
        // The same cannot be done for `validateTransaction` due to the many safeguards around
        // correct nonce update in the bootloader. So we handle it by modifying the storage
        // post-execution.
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
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
            let current = state.vm_local_state.callstack.current;

            if current.code_address == SYSTEM_CONTEXT_ADDRESS {
                if calldata.starts_with(&SELECTOR_SYSTEM_CONTEXT_BLOCK_NUMBER) {
                    self.farcall_handler
                        .set_immediate_return(self.call_context.block_number.to_be_bytes_vec());
                    return;
                } else if calldata.starts_with(&SELECTOR_SYSTEM_CONTEXT_BLOCK_TIMESTAMP) {
                    self.farcall_handler
                        .set_immediate_return(self.call_context.block_timestamp.to_be_bytes_vec());
                    return;
                }
            }
        }

        // Override block base fee for the transaction. This is properly setup when creating
        // `L1BatchEnv` but a value of `0` is auto-translated to `1`, so we ensure that it will
        // always be `0`.
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let current = state.vm_local_state.callstack.current;

            if current.code_address == SYSTEM_CONTEXT_ADDRESS &&
                calldata.starts_with(&SELECTOR_BASE_FEE)
            {
                self.farcall_handler
                    .set_immediate_return(self.call_context.block_basefee.to_be_bytes_vec());
                return;
            }
        }

        // Override blockhash
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
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

        // record immutables for an address during creates
        if self.call_context.is_create {
            if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
                let current = state.vm_local_state.callstack.current;

                if current.code_address == IMMUTABLE_SIMULATOR_STORAGE_ADDRESS &&
                    calldata.starts_with(&SELECTOR_IMMUTABLE_SIMULATOR_SET)
                {
                    let mut params = ethabi::decode(
                        &[
                            ethabi::ParamType::Address,
                            ethabi::ParamType::Array(Box::new(ethabi::ParamType::Tuple(vec![
                                ethabi::ParamType::Uint(256),
                                ethabi::ParamType::FixedBytes(32),
                            ]))),
                        ],
                        &calldata[4..],
                    )
                    .expect("failed decoding setImmutables parameters");

                    let address = params.remove(0).into_address().expect("must be valid address");
                    let immutables = params.remove(0).into_array().expect("must be valid array");
                    for immutable in immutables {
                        let mut imm_tuple = immutable.into_tuple().expect("must be valid tuple");
                        let imm_index =
                            imm_tuple.remove(0).into_uint().expect("must be valid uint").to_ru256();
                        let imm_value = imm_tuple
                            .remove(0)
                            .into_fixed_bytes()
                            .expect("must be valid fixed bytes");
                        let imm_value = FixedBytes::<32>::from_slice(&imm_value);

                        self.recorded_immutables
                            .entry(address)
                            .and_modify(|entry| {
                                entry.insert(imm_index, imm_value);
                            })
                            .or_insert_with(|| {
                                let mut value = HashMap::default();
                                value.insert(imm_index, imm_value);
                                value
                            });
                    }
                }
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

impl<S: WriteStorage + StorageViewRecorder, H: HistoryMode> VmTracer<S, H> for CheatcodeTracer {
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
        _stop_reason: zksync_multivm::interface::tracer::VmExecutionStopReason,
    ) {
        let cell = self.result.as_ref();
        cell.set(CheatcodeTracerResult {
            expected_calls: self.expected_calls.clone(),
            recorded_immutables: self.recorded_immutables.clone(),
        })
        .unwrap();
    }
}

pub(crate) fn get_calldata<H: HistoryMode>(
    state: &VmLocalStateData<'_>,
    memory: &SimpleMemory<H>,
) -> Vec<u8> {
    let ptr = state.vm_local_state.registers[CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER as usize];
    assert!(ptr.is_pointer);
    let fat_data_pointer = FatPointer::from_u256(ptr.value);
    memory.read_unaligned_bytes(
        fat_data_pointer.memory_page as usize,
        fat_data_pointer.start as usize,
        fat_data_pointer.length as usize,
    )
}
