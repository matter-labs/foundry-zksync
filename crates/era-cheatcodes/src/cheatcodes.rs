use crate::utils::{ToH160, ToH256};
use alloy_sol_types::SolInterface;
use era_test_node::{fork::ForkStorage, utils::bytecode_to_factory_dep};
use ethers::{
    abi::{AbiDecode, AbiEncode},
    utils::to_checksum,
};
use foundry_cheatcodes_spec::Vm;
use foundry_evm_core::{backend::DatabaseExt, era_revm::db::RevmDatabaseForEra};
use itertools::Itertools;
use multivm::{
    interface::{dyn_tracers::vm_1_3_3::DynTracer, tracer::TracerExecutionStatus, VmRevertReason},
    vm_refunds_enhancement::{BootloaderState, HistoryMode, SimpleMemory, VmTracer, ZkSyncVmState},
    zk_evm_1_3_3::{
        tracing::{AfterExecutionData, VmLocalStateData},
        vm_state::{PrimitiveValue, VmLocalState},
        zkevm_opcode_defs::{
            self,
            decoding::{EncodingModeProduction, VmEncodingMode},
            FatPointer, Opcode, RetOpcode, CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER,
            RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER,
        },
    },
};
use std::{cell::RefMut, collections::HashMap, fmt::Debug};
use zksync_basic_types::{AccountTreeId, H160, H256, U256};
use zksync_state::{ReadStorage, StoragePtr, StorageView, WriteStorage};
use zksync_types::{
    block::{pack_block_info, unpack_block_info},
    get_code_key, get_nonce_key,
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
    LogQuery, StorageKey, Timestamp,
};
use zksync_utils::{h256_to_u256, u256_to_h256};

type EraDb<DB> = StorageView<ForkStorage<RevmDatabaseForEra<DB>>>;
type PcOrImm = <EncodingModeProduction as VmEncodingMode<8>>::PcOrImm;

// address(uint160(uint256(keccak256('hevm cheat code'))))
const CHEATCODE_ADDRESS: H160 = H160([
    113, 9, 112, 158, 207, 169, 26, 128, 98, 111, 243, 152, 157, 104, 246, 127, 91, 29, 209, 45,
]);

const INTERNAL_CONTRACT_ADDRESSES: [H160; 20] = [
    zksync_types::BOOTLOADER_ADDRESS,
    zksync_types::ACCOUNT_CODE_STORAGE_ADDRESS,
    zksync_types::NONCE_HOLDER_ADDRESS,
    zksync_types::KNOWN_CODES_STORAGE_ADDRESS,
    zksync_types::IMMUTABLE_SIMULATOR_STORAGE_ADDRESS,
    zksync_types::CONTRACT_DEPLOYER_ADDRESS,
    zksync_types::CONTRACT_FORCE_DEPLOYER_ADDRESS,
    zksync_types::L1_MESSENGER_ADDRESS,
    zksync_types::MSG_VALUE_SIMULATOR_ADDRESS,
    zksync_types::KECCAK256_PRECOMPILE_ADDRESS,
    zksync_types::L2_ETH_TOKEN_ADDRESS,
    zksync_types::SYSTEM_CONTEXT_ADDRESS,
    zksync_types::BOOTLOADER_UTILITIES_ADDRESS,
    zksync_types::EVENT_WRITER_ADDRESS,
    zksync_types::COMPRESSOR_ADDRESS,
    zksync_types::COMPLEX_UPGRADER_ADDRESS,
    zksync_types::ECRECOVER_PRECOMPILE_ADDRESS,
    zksync_types::SHA256_PRECOMPILE_ADDRESS,
    zksync_types::MINT_AND_BURN_ADDRESS,
    H160::zero(),
];

#[derive(Debug, Default, Clone)]
pub struct CheatcodeTracer {
    one_time_actions: Vec<FinishCycleOneTimeActions>,
    recurring_actions: Vec<FinishCycleRecurringAction>,
    delayed_actions: Vec<DelayedNextStatementAction>,
    permanent_actions: FinishCyclePermanentActions,
    return_data: Option<Vec<U256>>,
    return_ptr: Option<FatPointer>,
    near_calls: usize,
    serialized_objects: HashMap<String, String>,
}

#[derive(Debug, Clone)]
enum FinishCycleOneTimeActions {
    StorageWrite { key: StorageKey, read_value: H256, write_value: H256 },
    StoreFactoryDep { hash: U256, bytecode: Vec<U256> },
    ForceRevert { error: Vec<u8>, exception_handler: PcOrImm },
    ForceReturn { data: Vec<u8>, continue_pc: PcOrImm },
}

#[derive(Debug, Clone)]
struct DelayedNextStatementAction {
    /// Target depth where the next statement would be
    target_depth: usize,
    statements_to_skip_count: usize,
    /// Action to queue when the condition is satisfied
    action: FinishCycleRecurringAction,
}

#[derive(Debug, Clone)]
enum FinishCycleRecurringAction {
    ExpectRevert {
        reason: Option<Vec<u8>>,
        depth: usize,
        prev_continue_pc: Option<PcOrImm>,
        prev_exception_handler_pc: Option<PcOrImm>,
    },
}

#[derive(Debug, Default, Clone)]
struct FinishCyclePermanentActions {
    start_prank: Option<StartPrankOpts>,
}

#[derive(Debug, Clone)]
struct StartPrankOpts {
    sender: H160,
    origin: Option<H256>,
}

impl<S: DatabaseExt + Send, H: HistoryMode> DynTracer<EraDb<S>, SimpleMemory<H>>
    for CheatcodeTracer
{
    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: multivm::zk_evm_1_3_3::tracing::BeforeExecutionData,
        memory: &SimpleMemory<H>,
        _storage: StoragePtr<EraDb<S>>,
    ) {
        //store the current exception handler in expect revert
        // to be used to force a revert
        if let Some(FinishCycleRecurringAction::ExpectRevert {
            prev_exception_handler_pc,
            prev_continue_pc,
            ..
        }) = self.current_expect_revert()
        {
            if matches!(data.opcode.variant.opcode, Opcode::Ret(_)) {
                let current = state.vm_local_state.callstack.current;
                let is_to_label = data.opcode.variant.flags
                    [zkevm_opcode_defs::RET_TO_LABEL_BIT_IDX] &
                    current.is_local_frame;
                tracing::debug!(%is_to_label, ?current, "storing continuations");

                if is_to_label {
                    prev_continue_pc.replace(data.opcode.imm_0);
                } else {
                    prev_continue_pc.replace(current.pc);
                }
                prev_exception_handler_pc.replace(current.exception_handler_location);
            }
        }
        if let Opcode::Ret(call) = data.opcode.variant.opcode {
            println!("ret : {} {:?}", state.vm_local_state.callstack.depth(), call);
        }
    }

    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterExecutionData,
        memory: &SimpleMemory<H>,
        storage: StoragePtr<EraDb<S>>,
    ) {
        let current = state.vm_local_state.callstack.get_current_stack();
        // in `handle_action`, when true is returned the current action will
        // be kept in the queue
        let handle_recurring_action = |action: &FinishCycleRecurringAction| match action {
            FinishCycleRecurringAction::ExpectRevert {
                reason,
                depth,
                prev_exception_handler_pc: exception_handler,
                prev_continue_pc: continue_pc,
            } if state.vm_local_state.callstack.depth() < *depth => {
                let callstack_depth = state.vm_local_state.callstack.depth();

                match data.opcode.variant.opcode {
                    Opcode::Ret(op @ RetOpcode::Revert) => {
                        tracing::debug!(wanted = %depth, current_depth = %callstack_depth, opcode = ?data.opcode.variant.opcode, "expectRevert");
                        let (Some(exception_handler), Some(continue_pc)) =
                            (*exception_handler, *continue_pc)
                        else {
                            tracing::error!("exceptRevert missing stored continuations");
                            return false
                        };

                        let current_continue_pc = {
                            let current = state.vm_local_state.callstack.current;
                            let is_to_label = data.opcode.variant.flags
                                [zkevm_opcode_defs::RET_TO_LABEL_BIT_IDX] &
                                current.is_local_frame;

                            if is_to_label {
                                data.opcode.imm_0
                            } else {
                                current.pc
                            }
                        };
                        self.one_time_actions.push(
                            Self::handle_except_revert(reason.as_ref(), op, &state, memory)
                                .map(|_| FinishCycleOneTimeActions::ForceReturn {
                                    //dummy data
                                    data: // vec![0u8; 8192]
                                        [0xde, 0xad, 0xbe, 0xef].to_vec(),
                                    continue_pc,
                                })
                                .unwrap_or_else(|error| FinishCycleOneTimeActions::ForceRevert {
                                    error,
                                    exception_handler,
                                }),
                        );
                        false
                    }
                    Opcode::Ret(RetOpcode::Ok) => {
                        tracing::debug!(wanted = %depth, current_depth = %callstack_depth, opcode = ?data.opcode.variant.opcode, "expectRevert");
                        let (Some(exception_handler), Some(continue_pc)) =
                            (*exception_handler, *continue_pc)
                        else {
                            tracing::error!("exceptRevert missing stored continuations");
                            return false
                        };
                        if let Err(err) = Self::handle_except_revert(
                            reason.as_ref(),
                            RetOpcode::Ok,
                            &state,
                            memory,
                        ) {
                            tracing::error!(?err, "unexpected opcode");
                            self.one_time_actions.push(FinishCycleOneTimeActions::ForceRevert {
                                error: err,
                                exception_handler,
                            });
                        }
                        false
                    }
                    _ => true,
                }
            }
            _ => true,
        };
        self.recurring_actions.retain(handle_recurring_action);

        // process delayed actions after to avoid new recurring actions to be
        // executed immediately (thus nullifying the delay)
        let process_delayed_action = |action: &mut DelayedNextStatementAction| {
            if state.vm_local_state.callstack.depth() != action.target_depth {
                return true
            }
            if action.statements_to_skip_count != 0 {
                action.statements_to_skip_count -= 1;
                return true
            }

            tracing::debug!(?action, "delay completed");
            self.recurring_actions.push(action.action.clone());
            false
        };
        //skip delayed actions if a cheatcode is invoked
        if current.code_address != CHEATCODE_ADDRESS {
            self.delayed_actions.retain_mut(process_delayed_action);
        }

        if self.return_data.is_some() {
            if let Opcode::Ret(_call) = data.opcode.variant.opcode {
                if self.near_calls == 0 {
                    let ptr = state.vm_local_state.registers
                        [RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER as usize];
                    let fat_data_pointer = FatPointer::from_u256(ptr.value);
                    self.return_ptr = Some(fat_data_pointer);
                } else {
                    self.near_calls = self.near_calls.saturating_sub(1);
                }
            }
        }

        if let Opcode::NearCall(_call) = data.opcode.variant.opcode {
            println!("near call: {}", state.vm_local_state.callstack.depth());
            if self.return_data.is_some() {
                self.near_calls += 1;
            }
        }

        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            println!("far call: {}", state.vm_local_state.callstack.depth());
            if current.code_address != CHEATCODE_ADDRESS {
                return
            }
            if current.code_page.0 == 0 || current.ergs_remaining == 0 {
                tracing::error!("cheatcode triggered, but no calldata or ergs available");
                return
            }
            tracing::info!("near call: cheatcode triggered");
            let calldata = {
                let ptr = state.vm_local_state.registers
                    [CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER as usize];
                assert!(ptr.is_pointer);
                let fat_data_pointer = FatPointer::from_u256(ptr.value);
                memory.read_unaligned_bytes(
                    fat_data_pointer.memory_page as usize,
                    fat_data_pointer.start as usize,
                    fat_data_pointer.length as usize,
                )
            };

            // try to dispatch the cheatcode
            if let Ok(call) = Vm::VmCalls::abi_decode(&calldata, true) {
                self.dispatch_cheatcode(state, data, memory, storage, call);
            } else {
                tracing::error!(
                    "Failed to decode cheatcode calldata (far call): {}",
                    hex::encode(calldata),
                );
            }
        }
    }
}

impl<S: DatabaseExt + Send, H: HistoryMode> VmTracer<EraDb<S>, H> for CheatcodeTracer {
    fn finish_cycle(
        &mut self,
        state: &mut ZkSyncVmState<EraDb<S>, H>,
        _bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        while let Some(action) = self.one_time_actions.pop() {
            match action {
                FinishCycleOneTimeActions::StorageWrite { key, read_value, write_value } => {
                    state.storage.write_value(LogQuery {
                        timestamp: Timestamp(state.local_state.timestamp),
                        tx_number_in_block: state.local_state.tx_number_in_block,
                        aux_byte: Default::default(),
                        shard_id: Default::default(),
                        address: *key.address(),
                        key: h256_to_u256(*key.key()),
                        read_value: h256_to_u256(read_value),
                        written_value: h256_to_u256(write_value),
                        rw_flag: true,
                        rollback: false,
                        is_service: false,
                    });
                }
                FinishCycleOneTimeActions::StoreFactoryDep { hash, bytecode } => state
                    .decommittment_processor
                    .populate(vec![(hash, bytecode)], Timestamp(state.local_state.timestamp)),
                FinishCycleOneTimeActions::ForceReturn { mut data, continue_pc: pc } => {
                    tracing::warn!("!!!! FORCING RETURN");

                    //TODO: override return data with the given one and force return (instead of
                    // revert)
                    self.add_trimmed_return_data(data.as_slice());
                    let ptr = state.local_state.registers
                        [RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER as usize];
                    let fat_data_pointer = FatPointer::from_u256(ptr.value);

                    Self::set_return(
                        fat_data_pointer,
                        self.return_data.take().unwrap(),
                        &mut state.local_state,
                        &mut state.memory,
                    );

                    // self.is_triggered_this_cycle = false;

                    //change current stack pc to label
                    state.local_state.callstack.get_current_stack_mut().pc = pc;
                    state.local_state.pending_exception = false;

                    // return TracerExecutionStatus::Continue
                }
                FinishCycleOneTimeActions::ForceRevert { error, exception_handler: pc } => {
                    use multivm::interface::{
                        tracer::TracerExecutionStopReason, Halt, VmRevertReason,
                    };

                    tracing::warn!("!!! FORCING REVERT");

                    self.add_trimmed_return_data(error.as_slice());
                    let ptr = state.local_state.registers
                        [RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER as usize];
                    let fat_data_pointer = FatPointer::from_u256(ptr.value);

                    Self::set_return(
                        fat_data_pointer,
                        self.return_data.take().unwrap(),
                        &mut state.local_state,
                        &mut state.memory,
                    );

                    // self.is_triggered_this_cycle = false;
                    //change current stack pc to exception handler
                    state.local_state.callstack.get_current_stack_mut().pc = pc;
                    // state.local_state.pending_exception = true;

                    // return TracerExecutionStatus::Stop(TracerExecutionStopReason::Abort(
                    //     Halt::Unknown(VmRevertReason::from(error.as_slice())),
                    // ))
                    // return TracerExecutionStatus::Continue
                }
            }
        }

        // Set return data, if any
        if let Some(mut fat_pointer) = self.return_ptr.take() {
            let elements = self.return_data.take().unwrap();

            Self::set_return(fat_pointer, elements, &mut state.local_state, &mut state.memory);
        }

        // Sets the sender address for startPrank cheatcode
        if let Some(start_prank_call) = &self.permanent_actions.start_prank {
            let this_address = state.local_state.callstack.current.this_address;
            if !INTERNAL_CONTRACT_ADDRESSES.contains(&this_address) {
                state.local_state.callstack.current.msg_sender = start_prank_call.sender;
            }
        }

        TracerExecutionStatus::Continue
    }
}

impl CheatcodeTracer {
    pub fn new() -> Self {
        CheatcodeTracer {
            one_time_actions: vec![],
            delayed_actions: vec![],
            recurring_actions: vec![],
            permanent_actions: FinishCyclePermanentActions { start_prank: None },
            near_calls: 0,
            return_data: None,
            return_ptr: None,
            serialized_objects: HashMap::new(),
        }
    }

    pub fn dispatch_cheatcode<S: DatabaseExt + Send, H: HistoryMode>(
        &mut self,
        state: VmLocalStateData<'_>,
        _data: AfterExecutionData,
        _memory: &SimpleMemory<H>,
        storage: StoragePtr<EraDb<S>>,
        call: Vm::VmCalls,
    ) {
        use Vm::{VmCalls::*, *};

        match call {
            addr(addrCall { privateKey: private_key }) => {
                tracing::info!("👷 Getting address for private key");
                let Ok(address) = zksync_types::PackedEthSignature::address_from_private_key(
                    &private_key.to_h256(),
                ) else {
                    tracing::error!("Failed generating address for private key");
                    return
                };
                self.return_data = Some(vec![h256_to_u256(address.into())]);
            }
            deal(dealCall { account, newBalance: new_balance }) => {
                tracing::info!("👷 Setting balance for {account:?} to {new_balance}");
                self.write_storage(
                    storage_key_for_eth_balance(&account.to_h160()),
                    new_balance.to_h256(),
                    &mut storage.borrow_mut(),
                );
            }
            etch(etchCall { target, newRuntimeBytecode: new_runtime_bytecode }) => {
                tracing::info!("👷 Setting address code for {target:?}");
                let code_key = get_code_key(&target.to_h160());
                let (hash, code) = bytecode_to_factory_dep(new_runtime_bytecode);
                self.store_factory_dep(hash, code);
                self.write_storage(code_key, u256_to_h256(hash), &mut storage.borrow_mut());
            }
            expectRevert_0(expectRevert_0Call {}) => {
                let callstack = state.vm_local_state.callstack.get_current_stack();
                let depth = state.vm_local_state.callstack.depth();
                tracing::info!(%depth, "👷 Setting up expectRevert for any reason");
                self.add_except_revert(None, depth)
            }
            expectRevert_1(expectRevert_1Call { revertData }) => {
                let callstack = state.vm_local_state.callstack.get_current_stack();
                let depth = state.vm_local_state.callstack.depth();
                tracing::info!(%depth, reason = ?revertData, "👷 Setting up expectRevert with bytes4 reason");
                self.add_except_revert(Some(revertData.to_vec()), depth)
            }
            expectRevert_2(expectRevert_2Call { revertData }) => {
                let callstack = state.vm_local_state.callstack.get_current_stack();
                let depth = state.vm_local_state.callstack.depth();
                tracing::info!(%depth, reason = ?revertData, "👷 Setting up expectRevert with reason");
                self.add_except_revert(Some(revertData.to_vec()), depth)
            }
            getNonce_0(getNonce_0Call { account }) => {
                tracing::info!("👷 Getting nonce for {account:?}");
                let mut storage = storage.borrow_mut();
                let nonce_key = get_nonce_key(&account.to_h160());
                let full_nonce = storage.read_value(&nonce_key);
                let (account_nonce, _) = decompose_full_nonce(h256_to_u256(full_nonce));
                tracing::info!(
                    "👷 Nonces for account {:?} are {}",
                    account,
                    account_nonce.as_u64()
                );
                tracing::info!("👷 Setting returndata",);
                tracing::info!("👷 Returndata is {:?}", account_nonce);
                self.return_data = Some(vec![account_nonce]);
            }
            load(loadCall { target, slot }) => {
                tracing::info!("👷 Getting storage slot {:?} for account {:?}", slot, target);
                let key = StorageKey::new(AccountTreeId::new(target.to_h160()), H256(*slot));
                let mut storage = storage.borrow_mut();
                let value = storage.read_value(&key);
                self.return_data = Some(vec![h256_to_u256(value)]);
            }
            roll(rollCall { newHeight: new_height }) => {
                tracing::info!("👷 Setting block number to {}", new_height);
                let key = StorageKey::new(
                    AccountTreeId::new(zksync_types::SYSTEM_CONTEXT_ADDRESS),
                    zksync_types::CURRENT_VIRTUAL_BLOCK_INFO_POSITION,
                );
                let mut storage = storage.borrow_mut();
                let (_, block_timestamp) =
                    unpack_block_info(h256_to_u256(storage.read_value(&key)));
                self.write_storage(
                    key,
                    u256_to_h256(pack_block_info(new_height.as_limbs()[0], block_timestamp)),
                    &mut storage,
                );
            }
            serializeAddress_0(serializeAddress_0Call {
                objectKey: object_key,
                valueKey: value_key,
                value,
            }) => {
                tracing::info!(
                    "👷 Serializing address {:?} with key {:?} to object {:?}",
                    value,
                    value_key,
                    object_key
                );
                let json_value = serde_json::json!({
                    value_key: value
                });

                //write to serialized_objects
                self.serialized_objects.insert(object_key.clone(), json_value.to_string());

                let address_with_checksum = to_checksum(&value.to_h160(), None);
                self.add_trimmed_return_data(address_with_checksum.as_bytes());
            }
            serializeBool_0(serializeBool_0Call {
                objectKey: object_key,
                valueKey: value_key,
                value,
            }) => {
                tracing::info!(
                    "👷 Serializing bool {:?} with key {:?} to object {:?}",
                    value,
                    value_key,
                    object_key
                );
                let json_value = serde_json::json!({
                    value_key: value
                });

                self.serialized_objects.insert(object_key.clone(), json_value.to_string());

                let bool_value = value.to_string();
                self.add_trimmed_return_data(bool_value.as_bytes());
            }
            serializeUint_0(serializeUint_0Call {
                objectKey: object_key,
                valueKey: value_key,
                value,
            }) => {
                tracing::info!(
                    "👷 Serializing uint256 {:?} with key {:?} to object {:?}",
                    value,
                    value_key,
                    object_key
                );
                let json_value = serde_json::json!({
                    value_key: value
                });

                self.serialized_objects.insert(object_key.clone(), json_value.to_string());

                let uint_value = value.to_string();
                self.add_trimmed_return_data(uint_value.as_bytes());
            }
            setNonce(setNonceCall { account, newNonce: new_nonce }) => {
                tracing::info!("👷 Setting nonce for {account:?} to {new_nonce}");
                let mut storage = storage.borrow_mut();
                let nonce_key = get_nonce_key(&account.to_h160());
                let full_nonce = storage.read_value(&nonce_key);
                let (mut account_nonce, mut deployment_nonce) =
                    decompose_full_nonce(h256_to_u256(full_nonce));
                if account_nonce.as_u64() >= new_nonce {
                    tracing::error!(
                      "SetNonce cheatcode failed: Account nonce is already set to a higher value ({}, requested {})",
                      account_nonce,
                      new_nonce
                  );
                    return
                }
                account_nonce = new_nonce.into();
                if deployment_nonce.as_u64() >= new_nonce {
                    tracing::error!(
                      "SetNonce cheatcode failed: Deployment nonce is already set to a higher value ({}, requested {})",
                      deployment_nonce,
                      new_nonce
                  );
                    return
                }
                deployment_nonce = new_nonce.into();
                let enforced_full_nonce = nonces_to_full_nonce(account_nonce, deployment_nonce);
                tracing::info!(
                    "👷 Nonces for account {:?} have been set to {}",
                    account,
                    new_nonce
                );
                self.write_storage(nonce_key, u256_to_h256(enforced_full_nonce), &mut storage);
            }
            startPrank_0(startPrank_0Call { msgSender: msg_sender }) => {
                tracing::info!("👷 Starting prank to {msg_sender:?}");
                self.permanent_actions.start_prank =
                    Some(StartPrankOpts { sender: msg_sender.to_h160(), origin: None });
            }
            startPrank_1(startPrank_1Call { msgSender: msg_sender, txOrigin: tx_origin }) => {
                tracing::info!("👷 Starting prank to {msg_sender:?} with origin {tx_origin:?}");
                let key = StorageKey::new(
                    AccountTreeId::new(zksync_types::SYSTEM_CONTEXT_ADDRESS),
                    zksync_types::SYSTEM_CONTEXT_TX_ORIGIN_POSITION,
                );
                let original_tx_origin = storage.borrow_mut().read_value(&key);
                self.write_storage(key, tx_origin.to_h160().into(), &mut storage.borrow_mut());

                self.permanent_actions.start_prank = Some(StartPrankOpts {
                    sender: msg_sender.to_h160(),
                    origin: Some(original_tx_origin),
                });
            }
            stopPrank(stopPrankCall {}) => {
                tracing::info!("👷 Stopping prank");

                if let Some(origin) =
                    self.permanent_actions.start_prank.as_ref().and_then(|v| v.origin)
                {
                    let key = StorageKey::new(
                        AccountTreeId::new(zksync_types::SYSTEM_CONTEXT_ADDRESS),
                        zksync_types::SYSTEM_CONTEXT_TX_ORIGIN_POSITION,
                    );
                    self.write_storage(key, origin, &mut storage.borrow_mut());
                }

                self.permanent_actions.start_prank = None;
            }
            store(storeCall { target, slot, value }) => {
                tracing::info!(
                    "👷 Setting storage slot {:?} for account {:?} to {:?}",
                    slot,
                    target,
                    value
                );
                let mut storage = storage.borrow_mut();
                let key = StorageKey::new(AccountTreeId::new(target.to_h160()), H256(*slot));
                self.write_storage(key, H256(*value), &mut storage);
            }
            toString_0(toString_0Call { value }) => {
                tracing::info!("Converting address into string");
                let address_with_checksum = to_checksum(&value.to_h160(), None);
                self.add_trimmed_return_data(address_with_checksum.as_bytes());
            }
            toString_1(toString_1Call { value }) => {
                tracing::info!("Converting bytes into string");
                let bytes_value = format!("0x{}", hex::encode(value));
                self.add_trimmed_return_data(bytes_value.as_bytes());
            }
            toString_2(toString_2Call { value }) => {
                tracing::info!("Converting bytes32 into string");
                let bytes_value = format!("0x{}", hex::encode(value));
                self.add_trimmed_return_data(bytes_value.as_bytes());
            }
            toString_3(toString_3Call { value }) => {
                tracing::info!("Converting bool into string");
                let bool_value = value.to_string();
                self.add_trimmed_return_data(bool_value.as_bytes());
            }
            toString_4(toString_4Call { value }) => {
                tracing::info!("Converting uint256 into string");
                let uint_value = value.to_string();
                self.add_trimmed_return_data(uint_value.as_bytes());
            }
            toString_5(toString_5Call { value }) => {
                tracing::info!("Converting int256 into string");
                let int_value = value.to_string();
                self.add_trimmed_return_data(int_value.as_bytes());
            }
            warp(warpCall { newTimestamp: new_timestamp }) => {
                tracing::info!("👷 Setting block timestamp {}", new_timestamp);

                let key = StorageKey::new(
                    AccountTreeId::new(zksync_types::SYSTEM_CONTEXT_ADDRESS),
                    zksync_types::CURRENT_VIRTUAL_BLOCK_INFO_POSITION,
                );
                let mut storage = storage.borrow_mut();
                let (block_number, _) = unpack_block_info(h256_to_u256(storage.read_value(&key)));
                self.write_storage(
                    key,
                    u256_to_h256(pack_block_info(block_number, new_timestamp.as_limbs()[0])),
                    &mut storage,
                );
            }
            _ => {
                tracing::error!("👷 Unrecognized cheatcode");
            }
        };
    }

    fn store_factory_dep(&mut self, hash: U256, bytecode: Vec<U256>) {
        self.one_time_actions.push(FinishCycleOneTimeActions::StoreFactoryDep { hash, bytecode });
    }

    fn write_storage<S: WriteStorage>(
        &mut self,
        key: StorageKey,
        write_value: H256,
        storage: &mut RefMut<S>,
    ) {
        self.one_time_actions.push(FinishCycleOneTimeActions::StorageWrite {
            key,
            read_value: storage.read_value(&key),
            write_value,
        });
    }

    fn add_trimmed_return_data(&mut self, data: &[u8]) {
        let data_length = data.len();
        let mut data: Vec<U256> = data
            .chunks(32)
            .map(|b| {
                // Copies the bytes into a 32 byte array
                // padding with zeros to the right if necessary
                let mut bytes = [0u8; 32];
                bytes[..b.len()].copy_from_slice(b);
                bytes.into()
            })
            .collect_vec();

        // Add the length of the data to the end of the return data
        data.push(data_length.into());

        self.return_data = Some(data);
    }

    fn set_return<H: HistoryMode>(
        mut fat_pointer: FatPointer,
        elements: Vec<U256>,
        state: &mut VmLocalState,
        memory: &mut SimpleMemory<H>,
    ) {
        let timestamp = Timestamp(state.timestamp);

        fat_pointer.length = (elements.len() as u32) * 32;
        state.registers[RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER as usize] =
            PrimitiveValue { value: fat_pointer.to_u256(), is_pointer: true };
        memory.populate_page(
            fat_pointer.memory_page as usize,
            elements.into_iter().enumerate().collect_vec(),
            timestamp,
        );
    }

    fn current_expect_revert(&mut self) -> Option<&mut FinishCycleRecurringAction> {
        let delayed_expect_revert = self
            .delayed_actions
            .iter_mut()
            .find(|action| matches!(action.action, FinishCycleRecurringAction::ExpectRevert { .. }))
            .map(|act| &mut act.action);

        delayed_expect_revert.or_else(|| {
            self.recurring_actions
                .iter_mut()
                .find(|act| matches!(act, FinishCycleRecurringAction::ExpectRevert { .. }))
        })
    }

    fn add_except_revert(&mut self, reason: Option<Vec<u8>>, depth: usize) {
        //TODO: check if an expect revert is already set

        //-2: possibly for EfficientCall.call EfficientCall.rawCall ? not confirmed
        let action = FinishCycleRecurringAction::ExpectRevert {
            reason,
            depth: depth - 2,
            prev_exception_handler_pc: None,
            prev_continue_pc: None,
        };
        let delay = DelayedNextStatementAction {
            target_depth: depth - 2,
            statements_to_skip_count: 0,
            action,
        };
        self.delayed_actions.push(delay);
    }

    fn handle_except_revert<H: HistoryMode>(
        reason: Option<&Vec<u8>>,
        op: zkevm_opcode_defs::RetOpcode,
        state: &VmLocalStateData<'_>,
        memory: &SimpleMemory<H>,
    ) -> Result<(), Vec<u8>> {
        println!("handle except revert {:?}, {:?}", &reason, &op);
        match (op, reason) {
            (zkevm_opcode_defs::RetOpcode::Revert, Some(expected_reason)) => {
                let retdata = {
                    let ptr = state.vm_local_state.registers
                        [RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER as usize];
                    assert!(ptr.is_pointer);
                    let fat_data_pointer = FatPointer::from_u256(ptr.value);
                    memory.read_unaligned_bytes(
                        fat_data_pointer.memory_page as usize,
                        fat_data_pointer.start as usize,
                        fat_data_pointer.length as usize,
                    )
                };

                tracing::debug!(?expected_reason, ?retdata);
                if !expected_reason.is_empty() && retdata.is_empty() {
                    return Err("call reverted as expected, but without data".to_string().into())
                }

                let actual_revert = match VmRevertReason::from(retdata.as_slice()) {
                    VmRevertReason::General { msg, data: _ } => msg,
                    _ => panic!("unexpected revert reason"),
                };

                let expected_reason = String::from_utf8_lossy(expected_reason).to_string();
                if &actual_revert == &expected_reason {
                    Ok(())
                } else {
                    Err(format!(
                        "Error != expected error: {} != {}",
                        &actual_revert, expected_reason,
                    )
                    .into())
                }
            }
            (zkevm_opcode_defs::RetOpcode::Revert, None) => {
                tracing::debug!("any revert accepted");
                Ok(())
            }
            (zkevm_opcode_defs::RetOpcode::Ok, _) => {
                tracing::debug!("expected revert but call succeeded");
                Err("expected revert but call succeeded".to_string().into())
            }
            (zkevm_opcode_defs::RetOpcode::Panic, _) => todo!("ignore/return error ?"),
        }
    }
}
