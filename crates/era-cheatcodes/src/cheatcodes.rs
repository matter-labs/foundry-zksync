use crate::utils::{ToH160, ToH256};
use alloy_sol_types::{SolInterface, SolValue};
use era_test_node::{fork::ForkStorage, utils::bytecode_to_factory_dep};
use ethers::utils::to_checksum;
use foundry_cheatcodes_spec::Vm;
use foundry_evm_core::{backend::DatabaseExt, era_revm::db::RevmDatabaseForEra};
use itertools::Itertools;
use multivm::{
    interface::{dyn_tracers::vm_1_3_3::DynTracer, tracer::TracerExecutionStatus},
    vm_refunds_enhancement::{BootloaderState, HistoryMode, SimpleMemory, VmTracer, ZkSyncVmState},
    zk_evm_1_3_3::{
        tracing::{AfterExecutionData, VmLocalStateData},
        vm_state::{CallStackEntry, PrimitiveValue},
        zkevm_opcode_defs::{
            FatPointer, Opcode, CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER,
            RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER,
        },
    },
};
use serde::Serialize;
use std::{
    cell::RefMut,
    collections::{HashMap, HashSet},
    fmt::Debug,
    fs,
    process::Command,
};
use zksync_basic_types::{AccountTreeId, H160, H256, U256};
use zksync_state::{ReadStorage, StoragePtr, StorageView, WriteStorage};
use zksync_types::{
    block::{pack_block_info, unpack_block_info},
    get_code_key, get_nonce_key,
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
    EventMessage, LogQuery, StorageKey, Timestamp,
};
use zksync_utils::{h256_to_u256, u256_to_h256};

type EraDb<DB> = StorageView<ForkStorage<RevmDatabaseForEra<DB>>>;

// address(uint160(uint256(keccak256('hevm cheat code'))))
const CHEATCODE_ADDRESS: H160 = H160([
    113, 9, 112, 158, 207, 169, 26, 128, 98, 111, 243, 152, 157, 104, 246, 127, 91, 29, 209, 45,
]);

// 0x2e1908b13b8b625ed13ecf03c87d45c499d1f325
const TEST_ADDRESS: H160 =
    H160([46, 25, 8, 177, 59, 139, 98, 94, 209, 62, 207, 3, 200, 125, 69, 196, 153, 209, 243, 37]);

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
    permanent_actions: FinishCyclePermanentActions,
    return_data: Option<Vec<U256>>,
    return_ptr: Option<FatPointer>,
    near_calls: usize,
    serialized_objects: HashMap<String, String>,
    recorded_logs: HashSet<LogEntry>,
    recording_logs: bool,
    recording_timestamp: u32,
    expected_calls: Vec<ExpectCallOpts>,
    expected_calls_checked: bool,
    test_status: TestStatus,
    callstacks: Vec<CallStackEntry>,
    test_callstacks: Vec<u64>,
    test_number: u64,
    callstack_depth: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Eq, Hash, PartialEq)]
struct LogEntry {
    topic: H256,
    data: H256,
    emitter: H160,
}

impl LogEntry {
    fn new(topic: H256, data: H256, emitter: H160) -> Self {
        LogEntry { topic, data, emitter }
    }
}

#[derive(Debug, Clone)]
enum FinishCycleOneTimeActions {
    StorageWrite { key: StorageKey, read_value: H256, write_value: H256 },
    StoreFactoryDep { hash: U256, bytecode: Vec<U256> },
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

#[derive(Debug, Clone)]
struct ExpectCallOpts {
    callee: H160,
    data: Vec<u8>,
    matches_found: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum TestStatus {
    #[default]
    NotStarted,
    Running {
        call_depth: usize,
    },
    Finished,
}

impl<S: DatabaseExt + Send, H: HistoryMode> DynTracer<EraDb<S>, SimpleMemory<H>>
    for CheatcodeTracer
{
    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterExecutionData,
        memory: &SimpleMemory<H>,
        storage: StoragePtr<EraDb<S>>,
    ) {
        let calldata = get_calldata(&state, memory);

        // Keep track of calls to detect if test started and stuff
        match data.opcode.variant.opcode {
            Opcode::NearCall(_) => {
                let current = state.vm_local_state.callstack.current;
                // if current.code_address == TEST_ADDRESS {
                println!(
                    "// near_call {:?} depth {}",
                    current,
                    state.vm_local_state.callstack.depth()
                );
                println!("// calldata: {}", hex::encode(calldata));
                self.test_number += 1;
                self.test_callstacks.push(self.test_number);
                println!("// test_number: {}", self.test_number);
                self.callstacks.push(current);
                // }
            }
            Opcode::FarCall(_) => {
                let current = state.vm_local_state.callstack.current;
                // if current.code_address == TEST_ADDRESS {
                println!(
                    "// far_call {:?} depth {}",
                    current,
                    state.vm_local_state.callstack.depth()
                );
                println!("// calldata: {}", hex::encode(calldata));
                self.test_number += 1;
                self.test_callstacks.push(self.test_number);
                self.callstacks.push(current);
                println!("// test_number: {}", self.test_number);
                if self.test_status == TestStatus::NotStarted &&
                    current.code_address == TEST_ADDRESS
                {
                    println!("TEST STARTED");
                    self.test_status =
                        TestStatus::Running { call_depth: state.vm_local_state.callstack.depth() };
                }
                // }
            }
            Opcode::Ret(_) => {
                let current = state.vm_local_state.callstack.current;
                // if current.code_address == TEST_ADDRESS {
                println!(
                    "// ret_call {:?} depth {}",
                    current,
                    state.vm_local_state.callstack.depth()
                );
                println!("// calldata: {}", hex::encode(calldata));
                self.callstacks.pop();
                println!("// popped test_number: {:?}", self.test_callstacks.pop());
                println!("// test_number_length: {}", self.test_callstacks.len());
                if self.test_callstacks.len() < 10 {
                    println!("// test_callstacks: {:?}", self.test_callstacks);
                }

                // Check ret opcode has 2 lower depth than
                if let Some(callstack_depth) = self.callstack_depth {
                    if state.vm_local_state.callstack.depth() == callstack_depth - 4 {
                        println!("// TEST FINISHED FOR REAL");

                        for expected_call in &mut self.expected_calls {
                            if expected_call.matches_found == 0 {
                                println!("EXPECTED CALL NOT FOUND");
                                println!("callee {:?}", expected_call.callee);
                                println!("data {:?}", expected_call.data);
                                // panic!("Expected call not found");
                            }
                        }
                    }
                }

                // }
            }
            _ => {}
        }

        // Checks contract calls for expectCall cheatcode
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let current = state.vm_local_state.callstack.current;
            let calldata = get_calldata(&state, memory);

            for expected_call in &mut self.expected_calls {
                if expected_call.callee == current.code_address && expected_call.data == calldata {
                    println!("MATCH FOUND");
                    println!("current {:?}", current);
                    println!("calldata {:?}", calldata);
                    expected_call.matches_found += 1;
                    return
                }
            }
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
            if self.return_data.is_some() {
                self.near_calls += 1;
            }
        }

        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let current = state.vm_local_state.callstack.current;
            if current.code_address != CHEATCODE_ADDRESS {
                return
            }
            if current.code_page.0 == 0 || current.ergs_remaining == 0 {
                tracing::error!("cheatcode triggered, but no calldata or ergs available");
                return
            }
            tracing::info!("near call: cheatcode triggered");
            let calldata = get_calldata(&state, memory);

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
        let emitter = state.local_state.callstack.current.this_address;
        if self.recording_logs {
            let logs = transform_to_logs(
                state
                    .event_sink
                    .get_events_and_l2_l1_logs_after_timestamp(Timestamp(self.recording_timestamp))
                    .0,
                emitter,
            );
            if !logs.is_empty() {
                let mut unique_set: HashSet<LogEntry> = HashSet::new();

                // Filter out duplicates and extend the unique entries to the vector
                self.recorded_logs
                    .extend(logs.into_iter().filter(|log| unique_set.insert(log.clone())));
            }
        }
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
            }
        }

        // Set return data, if any
        if let Some(mut fat_pointer) = self.return_ptr.take() {
            let timestamp = Timestamp(state.local_state.timestamp);

            let elements = self.return_data.take().unwrap();
            fat_pointer.length = (elements.len() as u32) * 32;
            state.local_state.registers[RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER as usize] =
                PrimitiveValue { value: fat_pointer.to_u256(), is_pointer: true };
            state.memory.populate_page(
                fat_pointer.memory_page as usize,
                elements.into_iter().enumerate().collect_vec(),
                timestamp,
            );
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
            permanent_actions: FinishCyclePermanentActions { start_prank: None },
            near_calls: 0,
            return_data: None,
            return_ptr: None,
            serialized_objects: HashMap::new(),
            recorded_logs: HashSet::new(),
            recording_logs: false,
            recording_timestamp: 0,
            expected_calls: vec![],
            test_status: TestStatus::NotStarted,
            expected_calls_checked: false,
            callstacks: vec![],
            test_callstacks: vec![],
            test_number: 0,
            callstack_depth: None,
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
            expectCall_0(expectCall_0Call { callee, data }) => {
                tracing::info!("👷 Setting expected call to {callee:?}");
                self.expected_calls.push(ExpectCallOpts {
                    callee: callee.to_h160(),
                    data,
                    matches_found: 0,
                });
                self.callstack_depth = Some(state.vm_local_state.callstack.depth());
            }
            ffi(ffiCall { commandInput: command_input }) => {
                tracing::info!("👷 Running ffi: {command_input:?}");
                let Some(first_arg) = command_input.get(0) else {
                    tracing::error!("Failed to run ffi: no args");
                    return
                };
                // TODO: set directory to root
                let Ok(output) = Command::new(first_arg).args(&command_input[1..]).output() else {
                    tracing::error!("Failed to run ffi");
                    return
                };

                // The stdout might be encoded on valid hex, or it might just be a string,
                // so we need to determine which it is to avoid improperly encoding later.
                let Ok(trimmed_stdout) = String::from_utf8(output.stdout) else {
                    tracing::error!("Failed to parse ffi output");
                    return
                };
                let trimmed_stdout = trimmed_stdout.trim();
                let encoded_stdout =
                    if let Ok(hex) = hex::decode(trimmed_stdout.trim_start_matches("0x")) {
                        hex
                    } else {
                        trimmed_stdout.as_bytes().to_vec()
                    };

                self.add_trimmed_return_data(&encoded_stdout);
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
            getRecordedLogs(getRecordedLogsCall {}) => {
                tracing::info!("👷 Getting recorded logs");
                let logs: Vec<Log> = self
                    .recorded_logs
                    .iter()
                    .map(|log| Log {
                        topics: vec![log.topic.to_fixed_bytes().into()],
                        data: log.data.to_fixed_bytes().into(),
                        emitter: log.emitter.to_fixed_bytes().into(),
                    })
                    .collect_vec();

                let result = getRecordedLogsReturn { logs };

                let return_data: Vec<U256> =
                    result.logs.abi_encode().chunks(32).map(|b| b.into()).collect_vec();

                self.return_data = Some(return_data);

                //clean up logs
                self.recorded_logs = HashSet::new();
                //disable flag of recording logs
                self.recording_logs = false;
            }
            load(loadCall { target, slot }) => {
                tracing::info!("👷 Getting storage slot {:?} for account {:?}", slot, target);
                let key = StorageKey::new(AccountTreeId::new(target.to_h160()), H256(*slot));
                let mut storage = storage.borrow_mut();
                let value = storage.read_value(&key);
                self.return_data = Some(vec![h256_to_u256(value)]);
            }
            recordLogs(recordLogsCall {}) => {
                tracing::info!("👷 Recording logs");
                tracing::info!(
                    "👷 Logs will be with the timestamp {}",
                    state.vm_local_state.timestamp
                );

                self.recording_timestamp = state.vm_local_state.timestamp;
                self.recording_logs = true;
            }
            readCallers(readCallersCall {}) => {
                tracing::info!("👷 Reading callers");

                let current_origin = {
                    let key = StorageKey::new(
                        AccountTreeId::new(zksync_types::SYSTEM_CONTEXT_ADDRESS),
                        zksync_types::SYSTEM_CONTEXT_TX_ORIGIN_POSITION,
                    );

                    storage.borrow_mut().read_value(&key)
                };

                let mut mode = CallerMode::None;
                let mut new_caller = current_origin;

                if let Some(prank) = &self.permanent_actions.start_prank {
                    //TODO: vm.prank -> CallerMode::Prank
                    println!("PRANK");
                    mode = CallerMode::RecurrentPrank;
                    new_caller = prank.sender.into();
                }
                // TODO: vm.broadcast / vm.startBroadcast section
                // else if let Some(broadcast) = broadcast {
                //     mode = if broadcast.single_call {
                //         CallerMode::Broadcast
                //     } else {
                //         CallerMode::RecurrentBroadcast
                //     };
                //     new_caller = &broadcast.new_origin;
                //     new_origin = &broadcast.new_origin;
                // }

                let caller_mode = (mode as u8).into();
                let message_sender = h256_to_u256(new_caller);
                let tx_origin = h256_to_u256(current_origin);

                self.return_data = Some(vec![caller_mode, message_sender, tx_origin]);
            }
            readFile(readFileCall { path }) => {
                tracing::info!("👷 Reading file in path {}", path);
                let Ok(data) = fs::read(path) else {
                    tracing::error!("Failed to read file");
                    return
                };
                self.add_trimmed_return_data(&data);
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

            tryFfi(tryFfiCall { commandInput: command_input }) => {
                tracing::info!("👷 Running try ffi: {command_input:?}");
                let Some(first_arg) = command_input.get(0) else {
                    tracing::error!("Failed to run ffi: no args");
                    return
                };
                // TODO: set directory to root
                let Ok(output) = Command::new(first_arg).args(&command_input[1..]).output() else {
                    tracing::error!("Failed to run ffi");
                    return
                };

                // The stdout might be encoded on valid hex, or it might just be a string,
                // so we need to determine which it is to avoid improperly encoding later.
                let Ok(trimmed_stdout) = String::from_utf8(output.stdout) else {
                    tracing::error!("Failed to parse ffi output");
                    return
                };
                let trimmed_stdout = trimmed_stdout.trim();
                let encoded_stdout =
                    if let Ok(hex) = hex::decode(trimmed_stdout.trim_start_matches("0x")) {
                        hex
                    } else {
                        trimmed_stdout.as_bytes().to_vec()
                    };

                let ffi_result = FfiResult {
                    exitCode: output.status.code().unwrap_or(69), // Default from foundry
                    stdout: encoded_stdout,
                    stderr: output.stderr,
                };
                let encoded_ffi_result: Vec<u8> = ffi_result.abi_encode();
                let return_data: Vec<U256> =
                    encoded_ffi_result.chunks(32).map(|b| b.into()).collect_vec();
                self.return_data = Some(return_data);
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
            writeFile(writeFileCall { path, data }) => {
                tracing::info!("👷 Writing data to file in path {}", path);
                if fs::write(path, data).is_err() {
                    tracing::error!("Failed to write file");
                }
            }
            writeJson_0(writeJson_0Call { json, path }) => {
                tracing::info!("👷 Writing json data to file in path {}", path);
                let Ok(json) = serde_json::from_str::<serde_json::Value>(&json) else {
                    tracing::error!("Failed to parse json");
                    return
                };
                let Ok(formatted_json) = serde_json::to_string_pretty(&json) else {
                    tracing::error!("Failed to format json");
                    return
                };
                if fs::write(path, formatted_json).is_err() {
                    tracing::error!("Failed to write file");
                }
            }
            writeJson_1(writeJson_1Call { json, path, valueKey: value_key }) => {
                tracing::info!("👷 Writing json data to file in path {path} with key {value_key}");
                let Ok(file) = fs::read_to_string(&path) else {
                    tracing::error!("Failed to read file");
                    return
                };
                let Ok(mut file_json) = serde_json::from_str::<serde_json::Value>(&file) else {
                    tracing::error!("Failed to parse json");
                    return
                };
                let Ok(json) = serde_json::from_str::<serde_json::Value>(&json) else {
                    tracing::error!("Failed to parse json");
                    return
                };
                file_json[value_key] = json;
                let Ok(formatted_json) = serde_json::to_string_pretty(&file_json) else {
                    tracing::error!("Failed to format json");
                    return
                };
                if fs::write(path, formatted_json).is_err() {
                    tracing::error!("Failed to write file");
                }
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
}
fn transform_to_logs(events: Vec<EventMessage>, emitter: H160) -> Vec<LogEntry> {
    events
        .iter()
        .filter_map(|event| {
            if event.address == zksync_types::EVENT_WRITER_ADDRESS {
                Some(LogEntry::new(u256_to_h256(event.key), u256_to_h256(event.value), emitter))
            } else {
                None
            }
        })
        .collect()
}

fn get_calldata<H: HistoryMode>(state: &VmLocalStateData<'_>, memory: &SimpleMemory<H>) -> Vec<u8> {
    let ptr = state.vm_local_state.registers[CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER as usize];
    if !ptr.is_pointer {
        return vec![]
    }
    // assert!(ptr.is_pointer);
    let fat_data_pointer = FatPointer::from_u256(ptr.value);
    memory.read_unaligned_bytes(
        fat_data_pointer.memory_page as usize,
        fat_data_pointer.start as usize,
        fat_data_pointer.length as usize,
    )
}
