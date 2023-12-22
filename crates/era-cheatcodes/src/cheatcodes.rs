use crate::utils::{ToH160, ToH256, ToU256};
use alloy_sol_types::{SolInterface, SolValue};
use era_test_node::{
    deps::storage_view::StorageView, fork::ForkStorage, utils::bytecode_to_factory_dep,
};
use ethers::utils::to_checksum;
use foundry_cheatcodes::CheatsConfig;
use foundry_cheatcodes_spec::Vm;
use foundry_evm_core::{
    backend::DatabaseExt,
    era_revm::{db::RevmDatabaseForEra, transactions::storage_to_state},
    fork::CreateFork,
    opts::EvmOpts,
};
use itertools::Itertools;
use multivm::{
    interface::{dyn_tracers::vm_1_4_0::DynTracer, tracer::TracerExecutionStatus},
    vm_latest::{
        BootloaderState, HistoryMode, L1BatchEnv, SimpleMemory, SystemEnv, VmTracer, ZkSyncVmState,
    },
    zk_evm_1_4_0::{
        reference_impls::event_sink::EventMessage,
        tracing::{AfterExecutionData, VmLocalStateData},
        vm_state::PrimitiveValue,
        zkevm_opcode_defs::{
            FatPointer, Opcode, CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER,
            RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER,
        },
    },
};
use revm::{
    primitives::{BlockEnv, CfgEnv, Env, SpecId, U256 as rU256},
    JournaledState,
};
use serde::Serialize;
use std::{
    cell::{OnceCell, RefMut},
    collections::{hash_map::Entry, HashMap, HashSet},
    fmt::Debug,
    fs,
    process::Command,
    sync::Arc,
};
use zksync_basic_types::{AccountTreeId, H160, H256, U256};
use zksync_state::{ReadStorage, StoragePtr, WriteStorage};
use zksync_types::{
    block::{pack_block_info, unpack_block_info},
    get_code_key, get_nonce_key,
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
    LogQuery, StorageKey, Timestamp,
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

#[derive(Debug, Clone)]
struct EraEnv {
    l1_batch_env: L1BatchEnv,
    system_env: SystemEnv,
}

/// Represents the state of a foundry test function, i.e. functions
/// prefixed with "testXXX"
#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum FoundryTestState {
    /// The test function is not yet running
    #[default]
    NotStarted,
    /// The test function is now running at the specified call depth
    Running { call_depth: usize },
    /// The test function has finished executing
    Finished,
}

#[derive(Debug, Default, Clone)]
pub struct CheatcodeTracer {
    one_time_actions: Vec<FinishCycleOneTimeActions>,
    permanent_actions: FinishCyclePermanentActions,
    return_data: Option<Vec<U256>>,
    return_ptr: Option<FatPointer>,
    near_calls: usize,
    serialized_objects: HashMap<String, String>,
    env: OnceCell<EraEnv>,
    config: Arc<CheatsConfig>,
    recorded_logs: HashSet<LogEntry>,
    recording_logs: bool,
    recording_timestamp: u32,
    expected_calls: ExpectedCallsTracker,
    test_status: FoundryTestState,
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
    CreateSelectFork { url_or_alias: String, block_number: Option<u64> },
    CreateFork { url_or_alias: String, block_number: Option<u64> },
    SelectFork { fork_id: U256 },
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

/// Tracks the expected calls per address.
///
/// For each address, we track the expected calls per call data. We track it in such manner
/// so that we don't mix together calldatas that only contain selectors and calldatas that contain
/// selector and arguments (partial and full matches).
///
/// This then allows us to customize the matching behavior for each call data on the
/// `ExpectedCallData` struct and track how many times we've actually seen the call on the second
/// element of the tuple.
type ExpectedCallsTracker = HashMap<H160, HashMap<Vec<u8>, (ExpectedCallData, u64)>>;

#[derive(Debug, Clone)]
struct ExpectedCallData {
    /// The expected value sent in the call
    value: Option<U256>,
    /// The number of times the call is expected to be made.
    /// If the type of call is `NonCount`, this is the lower bound for the number of calls
    /// that must be seen.
    /// If the type of call is `Count`, this is the exact number of calls that must be seen.
    count: u64,
    /// The type of expected call.
    call_type: ExpectedCallType,
}

/// The type of expected call.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ExpectedCallType {
    /// The call is expected to be made at least once.
    NonCount,
    /// The exact number of calls expected.
    Count,
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
        if self.update_test_status(&state, &data) == &FoundryTestState::Finished {
            for (address, expected_calls_for_target) in &self.expected_calls {
                for (expected_calldata, (expected, actual_count)) in expected_calls_for_target {
                    let failed = match expected.call_type {
                        // If the cheatcode was called with a `count` argument,
                        // we must check that the EVM performed a CALL with this calldata exactly
                        // `count` times.
                        ExpectedCallType::Count => expected.count != *actual_count,
                        // If the cheatcode was called without a `count` argument,
                        // we must check that the EVM performed a CALL with this calldata at least
                        // `count` times. The amount of times to check was
                        // the amount of time the cheatcode was called.
                        ExpectedCallType::NonCount => expected.count > *actual_count,
                    };
                    // TODO: change to proper revert
                    assert!(
                        !failed,
                        "Expected call to {:?} with data {:?} was found {} times, expected {}",
                        address, expected_calldata, actual_count, expected.count
                    );
                }
            }

            // reset the test state to avoid checking again
            self.reset_test_status();
        }

        // Checks contract calls for expectCall cheatcode
        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let current = state.vm_local_state.callstack.current;
            if let Some(expected_calls_for_target) =
                self.expected_calls.get_mut(&current.code_address)
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
                                 value == current.context_u128_value.into()})
                    {
                        *actual_count += 1;
                    }
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
            tracing::info!("far call: cheatcode triggered");
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
    fn initialize_tracer(
        &mut self,
        _state: &mut ZkSyncVmState<EraDb<S>, H>,
        l1_batch_env: &L1BatchEnv,
        system_env: &SystemEnv,
    ) {
        self.env
            .set(EraEnv { l1_batch_env: l1_batch_env.clone(), system_env: system_env.clone() })
            .unwrap();
    }

    fn finish_cycle(
        &mut self,
        state: &mut ZkSyncVmState<EraDb<S>, H>,
        bootloader_state: &mut BootloaderState,
        storage: StoragePtr<EraDb<S>>,
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
                FinishCycleOneTimeActions::CreateSelectFork { url_or_alias, block_number } => {
                    let modified_storage: HashMap<StorageKey, H256> = storage
                        .borrow_mut()
                        .modified_storage_keys()
                        .clone()
                        .into_iter()
                        .filter(|(key, _)| key.address() != &zksync_types::SYSTEM_CONTEXT_ADDRESS)
                        .collect();
                    storage.borrow_mut().clean_cache();
                    let fork_id = {
                        let handle: &ForkStorage<RevmDatabaseForEra<S>> =
                            &storage.borrow_mut().storage_handle;
                        let mut fork_storage = handle.inner.write().unwrap();
                        fork_storage.value_read_cache.clear();
                        let era_db = fork_storage.fork.as_ref().unwrap().fork_source.clone();
                        let bytecodes = bootloader_state
                            .get_last_tx_compressed_bytecodes()
                            .iter()
                            .map(|b| bytecode_to_factory_dep(b.original.clone()))
                            .collect();

                        let mut journaled_state = JournaledState::new(SpecId::LATEST, vec![]);
                        let state = storage_to_state(&era_db, &modified_storage, bytecodes);
                        *journaled_state.state() = state;

                        let mut db = era_db.db.lock().unwrap();
                        let era_env = self.env.get().unwrap();
                        let mut env = into_revm_env(era_env);
                        db.create_select_fork(
                            create_fork_request(
                                era_env,
                                self.config.clone(),
                                block_number,
                                &url_or_alias,
                            ),
                            &mut env,
                            &mut journaled_state,
                        )
                    };
                    storage.borrow_mut().modified_storage_keys = modified_storage;

                    self.return_data = Some(vec![fork_id.unwrap().to_u256()]);
                }
                FinishCycleOneTimeActions::CreateFork { url_or_alias, block_number } => {
                    let handle: &ForkStorage<RevmDatabaseForEra<S>> =
                        &storage.borrow_mut().storage_handle;
                    let era_db =
                        handle.inner.write().unwrap().fork.as_ref().unwrap().fork_source.clone();

                    let mut db = era_db.db.lock().unwrap();
                    let era_env = self.env.get().unwrap();
                    let fork_id = db.create_fork(create_fork_request(
                        era_env,
                        self.config.clone(),
                        block_number,
                        &url_or_alias,
                    ));
                    self.return_data = Some(vec![fork_id.unwrap().to_u256()]);
                }
                FinishCycleOneTimeActions::SelectFork { fork_id } => {
                    let modified_storage: HashMap<StorageKey, H256> = storage
                        .borrow_mut()
                        .modified_storage_keys()
                        .clone()
                        .into_iter()
                        .filter(|(key, _)| key.address() != &zksync_types::SYSTEM_CONTEXT_ADDRESS)
                        .collect();
                    {
                        storage.borrow_mut().clean_cache();
                        let handle: &ForkStorage<RevmDatabaseForEra<S>> =
                            &storage.borrow_mut().storage_handle;
                        let mut fork_storage = handle.inner.write().unwrap();
                        fork_storage.value_read_cache.clear();
                        let era_db = fork_storage.fork.as_ref().unwrap().fork_source.clone();
                        let bytecodes = bootloader_state
                            .get_last_tx_compressed_bytecodes()
                            .iter()
                            .map(|b| bytecode_to_factory_dep(b.original.clone()))
                            .collect();

                        let mut journaled_state = JournaledState::new(SpecId::LATEST, vec![]);
                        let state = storage_to_state(&era_db, &modified_storage, bytecodes);
                        *journaled_state.state() = state;

                        let mut db = era_db.db.lock().unwrap();
                        let era_env = self.env.get().unwrap();
                        let mut env = into_revm_env(era_env);
                        db.select_fork(
                            rU256::from(fork_id.as_u128()),
                            &mut env,
                            &mut journaled_state,
                        )
                        .unwrap();
                    }
                    storage.borrow_mut().modified_storage_keys = modified_storage;

                    self.return_data = Some(vec![fork_id]);
                }
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
    pub fn new(cheatcodes_config: Arc<CheatsConfig>) -> Self {
        Self { config: cheatcodes_config, ..Default::default() }
    }

    /// Resets the test state to [TestStatus::NotStarted]
    fn reset_test_status(&mut self) {
        self.test_status = FoundryTestState::NotStarted;
    }

    /// Updates and keeps track of the test status.
    ///
    /// A foundry test starting with "testXXX" prefix is said to running when it is first called
    /// with the test selector as calldata. The test finishes when the calldepth reaches the same
    /// depth as when it started, i.e. when the test function returns. The retun value is stored
    /// in the calldata.
    fn update_test_status(
        &mut self,
        state: &VmLocalStateData<'_>,
        data: &AfterExecutionData,
    ) -> &FoundryTestState {
        match data.opcode.variant.opcode {
            Opcode::FarCall(_) => {
                if self.test_status == FoundryTestState::NotStarted &&
                    state.vm_local_state.callstack.current.code_address == TEST_ADDRESS
                {
                    self.test_status = FoundryTestState::Running {
                        call_depth: state.vm_local_state.callstack.depth(),
                    };
                    tracing::info!("Test started");
                }
            }
            Opcode::Ret(_) => {
                if let FoundryTestState::Running { call_depth } = self.test_status {
                    // As we are checking the calldepth after execution, the stack has already been
                    // popped (so reduced by 1) and must be accounted for.
                    if call_depth == state.vm_local_state.callstack.depth() + 1 {
                        self.test_status = FoundryTestState::Finished;
                        tracing::info!("Test finished");
                    }
                }
            }
            _ => (),
        }

        &self.test_status
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
                self.expect_call(&callee.to_h160(), &data, None, 1, ExpectedCallType::NonCount);
            }
            expectCall_1(expectCall_1Call { callee, data, count }) => {
                tracing::info!("👷 Setting expected call to {callee:?} with count {count}");
                self.expect_call(&callee.to_h160(), &data, None, count, ExpectedCallType::Count);
            }
            expectCall_2(expectCall_2Call { callee, msgValue, data }) => {
                tracing::info!("👷 Setting expected call to {callee:?} with value {msgValue}");
                self.expect_call(
                    &callee.to_h160(),
                    &data,
                    Some(msgValue.to_u256()),
                    1,
                    ExpectedCallType::NonCount,
                );
            }
            expectCall_3(expectCall_3Call { callee, msgValue, data, count }) => {
                tracing::info!(
                    "👷 Setting expected call to {callee:?} with value {msgValue} and count
                {count}"
                );
                self.expect_call(
                    &callee.to_h160(),
                    &data,
                    Some(msgValue.to_u256()),
                    count,
                    ExpectedCallType::Count,
                );
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
            createSelectFork_0(createSelectFork_0Call { urlOrAlias }) => {
                tracing::info!("👷 Creating and selecting fork {}", urlOrAlias,);

                self.one_time_actions.push(FinishCycleOneTimeActions::CreateSelectFork {
                    url_or_alias: urlOrAlias,
                    block_number: None,
                });
            }
            createSelectFork_1(createSelectFork_1Call { urlOrAlias, blockNumber }) => {
                let block_number = blockNumber.to_u256().as_u64();
                tracing::info!(
                    "👷 Creating and selecting fork {} for block number {}",
                    urlOrAlias,
                    block_number
                );
                self.one_time_actions.push(FinishCycleOneTimeActions::CreateSelectFork {
                    url_or_alias: urlOrAlias,
                    block_number: Some(block_number),
                });
            }
            createFork_0(createFork_0Call { urlOrAlias }) => {
                tracing::info!("👷 Creating fork {}", urlOrAlias,);

                self.one_time_actions.push(FinishCycleOneTimeActions::CreateFork {
                    url_or_alias: urlOrAlias,
                    block_number: None,
                });
            }
            createFork_1(createFork_1Call { urlOrAlias, blockNumber }) => {
                let block_number = blockNumber.to_u256().as_u64();
                tracing::info!("👷 Creating fork {} for block number {}", urlOrAlias, block_number);
                self.one_time_actions.push(FinishCycleOneTimeActions::CreateFork {
                    url_or_alias: urlOrAlias,
                    block_number: Some(block_number),
                });
            }
            selectFork(selectForkCall { forkId }) => {
                tracing::info!("👷 Selecting fork {}", forkId);

                self.one_time_actions
                    .push(FinishCycleOneTimeActions::SelectFork { fork_id: forkId.to_u256() });
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

    /// Adds an expectCall to the tracker.
    #[allow(clippy::too_many_arguments)]
    fn expect_call(
        &mut self,
        callee: &H160,
        calldata: &Vec<u8>,
        value: Option<U256>,
        count: u64,
        call_type: ExpectedCallType,
    ) {
        let expecteds = self.expected_calls.entry(*callee).or_default();

        match call_type {
            ExpectedCallType::Count => {
                // Get the expected calls for this target.
                // In this case, as we're using counted expectCalls, we should not be able to set
                // them more than once.
                assert!(
                    !expecteds.contains_key(calldata),
                    "counted expected calls can only bet set once"
                );
                expecteds
                    .insert(calldata.to_vec(), (ExpectedCallData { value, count, call_type }, 0));
            }
            ExpectedCallType::NonCount => {
                // Check if the expected calldata exists.
                // If it does, increment the count by one as we expect to see it one more time.
                match expecteds.entry(calldata.clone()) {
                    Entry::Occupied(mut entry) => {
                        let (expected, _) = entry.get_mut();
                        // Ensure we're not overwriting a counted expectCall.
                        assert!(
                            expected.call_type == ExpectedCallType::NonCount,
                            "cannot overwrite a counted expectCall with a non-counted expectCall"
                        );
                        expected.count += 1;
                    }
                    // If it does not exist, then create it.
                    Entry::Vacant(entry) => {
                        entry.insert((ExpectedCallData { value, count, call_type }, 0));
                    }
                }
            }
        }
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

fn into_revm_env(env: &EraEnv) -> Env {
    use foundry_common::zk_utils::conversion_utils::h160_to_address;
    use revm::primitives::U256;
    let block = BlockEnv {
        number: U256::from(env.l1_batch_env.first_l2_block.number),
        coinbase: h160_to_address(env.l1_batch_env.fee_account),
        timestamp: U256::from(env.l1_batch_env.first_l2_block.timestamp),
        gas_limit: U256::from(env.system_env.gas_limit),
        basefee: U256::from(env.l1_batch_env.base_fee()),
        ..Default::default()
    };

    let mut cfg = CfgEnv::default();
    cfg.chain_id = env.system_env.chain_id.as_u64();

    Env { block, cfg, ..Default::default() }
}

fn create_fork_request(
    env: &EraEnv,
    config: Arc<CheatsConfig>,
    block_number: Option<u64>,
    url_or_alias: &str,
) -> CreateFork {
    use foundry_evm_core::opts::Env;
    use revm::primitives::Address as revmAddress;

    let url = config.rpc_url(url_or_alias).unwrap();
    let env = into_revm_env(env);
    let opts_env = Env {
        gas_limit: u64::MAX,
        chain_id: None,
        tx_origin: revmAddress::ZERO,
        block_number: 0,
        block_timestamp: 0,
        ..Default::default()
    };
    let evm_opts = EvmOpts {
        env: opts_env,
        fork_url: Some(url.clone()),
        fork_block_number: block_number,
        ..Default::default()
    };

    CreateFork {
        enable_caching: config.rpc_storage_caching.enable_for_endpoint(&url),
        url,
        env,
        evm_opts,
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
