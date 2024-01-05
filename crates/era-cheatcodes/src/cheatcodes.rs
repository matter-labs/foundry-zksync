use crate::{
    events::LogEntry,
    utils::{ToH160, ToH256, ToU256},
};
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
    interface::{dyn_tracers::vm_1_4_0::DynTracer, tracer::TracerExecutionStatus, VmRevertReason},
    vm_latest::{
        BootloaderState, HistoryMode, L1BatchEnv, SimpleMemory, SystemEnv, VmTracer, ZkSyncVmState,
    },
    zk_evm_1_4_0::{
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
use revm::{
    primitives::{ruint::Uint, BlockEnv, CfgEnv, Env, SpecId, U256 as rU256},
    JournaledState,
};
use serde::Serialize;
use std::{
    cell::{OnceCell, RefMut},
    collections::{hash_map::Entry, HashMap, HashSet},
    fmt::Debug,
    fs,
    ops::BitAnd,
    process::Command,
    str::FromStr,
    sync::Arc,
};
use zksync_basic_types::{AccountTreeId, H160, H256, U256};
use zksync_state::{ReadStorage, StoragePtr, WriteStorage};
use zksync_types::{
    block::{pack_block_info, unpack_block_info},
    get_code_key, get_nonce_key,
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
    LogQuery, StorageKey, Timestamp, ACCOUNT_CODE_STORAGE_ADDRESS,
};
use zksync_utils::{h256_to_u256, u256_to_h256};

type EraDb<DB> = StorageView<ForkStorage<RevmDatabaseForEra<DB>>>;
type PcOrImm = <EncodingModeProduction as VmEncodingMode<8>>::PcOrImm;

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
    next_return_action: Option<NextReturnAction>,
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
    emit_config: EmitConfig,
    saved_snapshots: HashMap<U256, SavedSnapshot>,
}

#[derive(Debug, Clone)]
pub struct SavedSnapshot {
    modified_storage: HashMap<StorageKey, H256>,
}

#[derive(Debug, Clone, Default)]
struct EmitConfig {
    expected_emit_state: ExpectedEmitState,
    expect_emits_since: u32,
    expect_emits_until: u32,
    call_emits_since: u32,
    call_emits_until: u32,
    call_depth: usize,
    checks: EmitChecks,
}

#[derive(Debug, Clone, Default)]
struct EmitChecks {
    address: Option<H160>,
    topics: [bool; 3],
    data: bool,
}

#[derive(Debug, Clone, Serialize, Eq, Hash, PartialEq, Default)]
enum ExpectedEmitState {
    #[default]
    NotStarted,
    ExpectedEmitTriggered,
    CallTriggered,
    Assert,
    Finished,
}

#[derive(Debug, Clone)]
enum FinishCycleOneTimeActions {
    StorageWrite { key: StorageKey, read_value: H256, write_value: H256 },
    StoreFactoryDep { hash: U256, bytecode: Vec<U256> },
    ForceRevert { error: Vec<u8>, exception_handler: PcOrImm },
    ForceReturn { data: Vec<u8>, continue_pc: PcOrImm },
    CreateSelectFork { url_or_alias: String, block_number: Option<u64> },
    CreateFork { url_or_alias: String, block_number: Option<u64> },
    SelectFork { fork_id: U256 },
    RevertToSnapshot { snapshot_id: U256 },
    Snapshot,
}

#[derive(Debug, Clone)]
struct NextReturnAction {
    /// Target depth where the next statement would be
    target_depth: usize,
    /// Action to queue when the condition is satisfied
    action: ActionOnReturn,
    returns_to_skip: usize,
}

#[derive(Debug, Clone)]
enum ActionOnReturn {
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
    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: multivm::zk_evm_1_4_0::tracing::BeforeExecutionData,
        _memory: &SimpleMemory<H>,
        _storage: StoragePtr<EraDb<S>>,
    ) {
        //store the current exception handler in expect revert
        // to be used to force a revert
        if let Some(ActionOnReturn::ExpectRevert {
            prev_exception_handler_pc,
            prev_continue_pc,
            ..
        }) = self.current_expect_revert()
        {
            if matches!(data.opcode.variant.opcode, Opcode::Ret(_)) {
                // Callstack on the desired depth, it has the correct pc for continue
                let last = state.vm_local_state.callstack.inner.last().unwrap();
                // Callstack on the current depth, it has the correct pc for exception handler and
                // is_local_frame
                let current = &state.vm_local_state.callstack.current;
                let is_to_label: bool = data.opcode.variant.flags
                    [zkevm_opcode_defs::RET_TO_LABEL_BIT_IDX] &
                    state.vm_local_state.callstack.current.is_local_frame;
                tracing::debug!(%is_to_label, ?last, "storing continuations");

                // The source https://github.com/matter-labs/era-zk_evm/blob/763ef5dfd52fecde36bfdd01d47589b61eabf118/src/opcodes/execution/ret.rs#L242
                if is_to_label {
                    prev_continue_pc.replace(data.opcode.imm_0);
                } else {
                    prev_continue_pc.replace(last.pc);
                }

                prev_exception_handler_pc.replace(current.exception_handler_location);
            }
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
        let is_reserved_addr = current
            .code_address
            .bitand(H160::from_str("ffffffffffffffffffffffffffffffffffff0000").unwrap())
            .is_zero();

        if current.code_address != CHEATCODE_ADDRESS &&
            !INTERNAL_CONTRACT_ADDRESSES.contains(&current.code_address) &&
            !is_reserved_addr
        {
            if self.emit_config.expected_emit_state == ExpectedEmitState::ExpectedEmitTriggered {
                //cheatcode triggered, waiting for far call
                if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
                    self.emit_config.call_emits_since = state.vm_local_state.timestamp;
                    self.emit_config.expect_emits_until = state.vm_local_state.timestamp;
                    self.emit_config.expected_emit_state = ExpectedEmitState::CallTriggered;
                    self.emit_config.call_depth = state.vm_local_state.callstack.depth();
                }
            }

            if self.emit_config.expected_emit_state == ExpectedEmitState::CallTriggered &&
                state.vm_local_state.callstack.depth() < self.emit_config.call_depth
            {
                self.emit_config.call_emits_until = state.vm_local_state.timestamp;
            }
        }

        if self.update_test_status(&state, &data) == &FoundryTestState::Finished {
            // Trigger assert for emit_logs
            self.emit_config.expected_emit_state = ExpectedEmitState::Assert;

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

        // Checks returns from caontracts for expectRevert cheatcode
        self.handle_return(&state, &data, memory);

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

        let current = state.vm_local_state.callstack.get_current_stack();

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
            if current.code_address == ACCOUNT_CODE_STORAGE_ADDRESS {
                if let Some(action) = &mut self.next_return_action {
                    // if the call is to the account storage contract, we need to skip the next
                    // return and our code assumes that we are working with return opcode, so we
                    // have to increase target depth
                    if action.target_depth + 1 == state.vm_local_state.callstack.depth() {
                        action.returns_to_skip += 1;
                    }
                }
            }

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
        if self.recording_logs {
            let (events, _) = state.event_sink.get_events_and_l2_l1_logs_after_timestamp(
                zksync_types::Timestamp(self.recording_timestamp),
            );
            let logs = crate::events::parse_events(events);
            //insert logs in the hashset
            for log in logs {
                self.recorded_logs.insert(log);
            }
        }

        // This assert is triggered only once after the test execution finishes
        // And is used to assert that all logs exist
        if self.emit_config.expected_emit_state == ExpectedEmitState::Assert {
            self.emit_config.expected_emit_state = ExpectedEmitState::Finished;

            let (expected_events_initial_dimension, _) =
                state.event_sink.get_events_and_l2_l1_logs_after_timestamp(
                    zksync_types::Timestamp(self.emit_config.expect_emits_since),
                );
            let expected_events_surplus = state
                .event_sink
                .get_events_and_l2_l1_logs_after_timestamp(zksync_types::Timestamp(
                    self.emit_config.expect_emits_until,
                ))
                .0
                .len();

            //remove n surplus events from the end of expected_events_initial_dimension
            let expected_events = expected_events_initial_dimension
                .clone()
                .into_iter()
                .take(expected_events_initial_dimension.len() - expected_events_surplus)
                .collect::<Vec<_>>();
            let expected_logs = crate::events::parse_events(expected_events);

            let (actual_events_initial_dimension, _) =
                state.event_sink.get_events_and_l2_l1_logs_after_timestamp(
                    zksync_types::Timestamp(self.emit_config.call_emits_since),
                );
            let actual_events_surplus = state
                .event_sink
                .get_events_and_l2_l1_logs_after_timestamp(zksync_types::Timestamp(
                    self.emit_config.call_emits_until,
                ))
                .0
                .len();

            //remove n surplus events from the end of actual_events_initial_dimension
            let actual_events = actual_events_initial_dimension
                .clone()
                .into_iter()
                .take(actual_events_initial_dimension.len() - actual_events_surplus)
                .collect::<Vec<_>>();
            let actual_logs = crate::events::parse_events(actual_events);

            assert!(compare_logs(&expected_logs, &actual_logs, self.emit_config.checks.clone()));
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
                FinishCycleOneTimeActions::RevertToSnapshot { snapshot_id } => {
                    let mut storage = storage.borrow_mut();

                    let modified_storage: HashMap<StorageKey, H256> = storage
                        .modified_storage_keys()
                        .clone()
                        .into_iter()
                        .filter(|(key, _)| key.address() != &zksync_types::SYSTEM_CONTEXT_ADDRESS)
                        .collect();

                    storage.clean_cache();

                    {
                        let handle: &ForkStorage<RevmDatabaseForEra<S>> = &storage.storage_handle;
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
                        db.revert(Uint::from_limbs(snapshot_id.0), &journaled_state, &mut env);
                    }

                    storage.modified_storage_keys =
                        self.saved_snapshots.remove(&snapshot_id).unwrap().modified_storage;
                }
                FinishCycleOneTimeActions::Snapshot => {
                    let mut storage = storage.borrow_mut();

                    let modified_storage: HashMap<StorageKey, H256> = storage
                        .modified_storage_keys()
                        .clone()
                        .into_iter()
                        .filter(|(key, _)| key.address() != &zksync_types::SYSTEM_CONTEXT_ADDRESS)
                        .collect();

                    storage.clean_cache();

                    let snapshot_id = {
                        let handle: &ForkStorage<RevmDatabaseForEra<S>> = &storage.storage_handle;
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
                        let env = into_revm_env(era_env);
                        let snapshot_id = db.snapshot(&journaled_state, &env);

                        self.saved_snapshots.insert(
                            snapshot_id.to_u256(),
                            SavedSnapshot { modified_storage: modified_storage.clone() },
                        );
                        snapshot_id
                    };

                    storage.modified_storage_keys = modified_storage;
                    self.return_data = Some(vec![snapshot_id.to_u256()]);
                }
                FinishCycleOneTimeActions::ForceReturn { data, continue_pc: pc } => {
                    tracing::debug!("!!!! FORCING RETURN");

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

                    //change current stack pc to label
                    state.local_state.callstack.get_current_stack_mut().pc = pc;
                }
                FinishCycleOneTimeActions::ForceRevert { error, exception_handler: pc } => {
                    tracing::debug!("!!! FORCING REVERT");

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

                    //change current stack pc to exception handler
                    state.local_state.callstack.get_current_stack_mut().pc = pc;
                }
            }
        }

        // Set return data, if any
        if let Some(fat_pointer) = self.return_ptr.take() {
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
                    tracing::info!("Test started depth {}", state.vm_local_state.callstack.depth());
                }
            }
            Opcode::Ret(_) => {
                if let FoundryTestState::Running { call_depth } = self.test_status {
                    // As we are checking the calldepth after execution, the stack has already been
                    // popped (so reduced by 1) and must be accounted for.
                    if call_depth == state.vm_local_state.callstack.depth() + 1 {
                        self.test_status = FoundryTestState::Finished;
                        tracing::info!("Test finished {}", state.vm_local_state.callstack.depth());
                        // panic!("Test finished")
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
            expectRevert_0(expectRevert_0Call {}) => {
                let depth = state.vm_local_state.callstack.depth();
                tracing::info!(%depth, "👷 Setting up expectRevert for any reason");
                self.add_expect_revert(None, depth)
            }
            expectRevert_1(expectRevert_1Call { revertData }) => {
                let depth = state.vm_local_state.callstack.depth();
                tracing::info!(%depth, reason = ?revertData, "👷 Setting up expectRevert with bytes4 reason");
                self.add_expect_revert(Some(revertData.to_vec()), depth)
            }
            expectRevert_2(expectRevert_2Call { revertData }) => {
                let depth = state.vm_local_state.callstack.depth();
                tracing::info!(%depth, reason = ?revertData, "👷 Setting up expectRevert with reason");
                self.add_expect_revert(Some(revertData.to_vec()), depth)
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
            expectEmit_0(expectEmit_0Call { checkTopic1, checkTopic2, checkTopic3, checkData }) => {
                tracing::info!(
                    "👷 Setting expected emit with checks {:?}, {:?}, {:?}, {:?}",
                    checkTopic1,
                    checkTopic2,
                    checkTopic3,
                    checkData
                );
                self.emit_config.expected_emit_state = ExpectedEmitState::ExpectedEmitTriggered;
                self.emit_config.expect_emits_since = state.vm_local_state.timestamp;
                self.emit_config.checks = EmitChecks {
                    address: None,
                    topics: [checkTopic1, checkTopic2, checkTopic3],
                    data: checkData,
                };
            }
            expectEmit_1(expectEmit_1Call {
                checkTopic1,
                checkTopic2,
                checkTopic3,
                checkData,
                emitter,
            }) => {
                tracing::info!(
                    "👷 Setting expected emit with checks {:?}, {:?}, {:?}, {:?} from emitter {:?}",
                    checkTopic1,
                    checkTopic2,
                    checkTopic3,
                    checkData,
                    emitter
                );
                self.emit_config.expected_emit_state = ExpectedEmitState::ExpectedEmitTriggered;
                self.emit_config.expect_emits_since = state.vm_local_state.timestamp;
                self.emit_config.checks = EmitChecks {
                    address: Some(emitter.to_h160()),
                    topics: [checkTopic1, checkTopic2, checkTopic3],
                    data: checkData,
                };
                self.emit_config.call_depth = state.vm_local_state.callstack.depth();
            }
            expectEmit_2(expectEmit_2Call {}) => {
                tracing::info!("👷 Setting expected emit at {}", state.vm_local_state.timestamp);
                self.emit_config.expected_emit_state = ExpectedEmitState::ExpectedEmitTriggered;
                self.emit_config.expect_emits_since = state.vm_local_state.timestamp;
                self.emit_config.checks =
                    EmitChecks { address: None, topics: [true; 3], data: true };
            }
            ffi(ffiCall { commandInput: command_input }) => {
                tracing::info!("👷 Running ffi: {command_input:?}");
                let Some(first_arg) = command_input.get(0) else {
                    tracing::error!("Failed to run ffi: no args");
                    return
                };
                let Ok(output) = Command::new(first_arg)
                    .args(&command_input[1..])
                    .current_dir(&self.config.root)
                    .output()
                else {
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
                    .filter(|log| !log.data.is_empty())
                    .map(|log| Log {
                        topics: log
                            .topics
                            .iter()
                            .map(|topic| topic.to_fixed_bytes().into())
                            .collect(),
                        data: log.data.clone(),
                        emitter: log.address.to_fixed_bytes().into(),
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
            revertTo(revertToCall { snapshotId }) => {
                tracing::info!("👷 Reverting to snapshot {}", snapshotId);
                self.one_time_actions.push(FinishCycleOneTimeActions::RevertToSnapshot {
                    snapshot_id: snapshotId.to_u256(),
                });
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
            rpcUrl(rpcUrlCall { rpcAlias }) => {
                tracing::info!("👷 Getting rpc url of {}", rpcAlias);
                let rpc_endpoints = &self.config.rpc_endpoints;
                let rpc_url = match rpc_endpoints.get(&rpcAlias) {
                    Some(Ok(url)) => Some(url.clone()),
                    _ => None,
                };
                //this should revert but we don't have reverts yet
                assert!(
                    rpc_url.is_some(),
                    "Failed to resolve env var `${rpcAlias}`: environment variable not found"
                );
                self.add_trimmed_return_data(rpc_url.unwrap().as_bytes());
            }
            rpcUrls(rpcUrlsCall {}) => {
                tracing::info!("👷 Getting rpc urls");
                let rpc_endpoints = &self.config.rpc_endpoints;
                let rpc_urls = rpc_endpoints
                    .iter()
                    .map(|(alias, url)| match url {
                        Ok(url) => format!("{}:{}", alias, url),
                        Err(_) => alias.clone(),
                    })
                    .collect::<Vec<String>>()
                    .join(",");
                self.add_trimmed_return_data(rpc_urls.as_bytes());
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
            snapshot(snapshotCall {}) => {
                tracing::info!("👷 Creating snapshot");
                self.one_time_actions.push(FinishCycleOneTimeActions::Snapshot);
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
                let Ok(output) = Command::new(first_arg)
                    .args(&command_input[1..])
                    .current_dir(&self.config.root)
                    .output()
                else {
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

    fn current_expect_revert(&mut self) -> Option<&mut ActionOnReturn> {
        self.next_return_action.as_mut().map(|action| &mut action.action)
    }

    fn add_expect_revert(&mut self, reason: Option<Vec<u8>>, depth: usize) {
        if self.current_expect_revert().is_some() {
            panic!("expectRevert already set")
        }

        //-1: Because we are working with return opcode and it pops the stack after execution
        let action = ActionOnReturn::ExpectRevert {
            reason,
            depth: depth - 1,
            prev_exception_handler_pc: None,
            prev_continue_pc: None,
        };

        // We have to skip at least one return from CHEATCODES contract
        self.next_return_action =
            Some(NextReturnAction { target_depth: depth - 1, action, returns_to_skip: 1 });
    }

    fn handle_except_revert<H: HistoryMode>(
        reason: Option<&Vec<u8>>,
        op: zkevm_opcode_defs::RetOpcode,
        state: &VmLocalStateData<'_>,
        memory: &SimpleMemory<H>,
    ) -> Result<(), Vec<u8>> {
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

                match VmRevertReason::from(retdata.as_slice()) {
                    VmRevertReason::General { msg, data: _ } => {
                        let expected_reason = String::from_utf8_lossy(expected_reason).to_string();
                        if msg == expected_reason {
                            Ok(())
                        } else {
                            Err(format!(
                                "Error != expected error: {} != {}",
                                &msg, expected_reason,
                            )
                            .into())
                        }
                    }
                    VmRevertReason::Unknown { function_selector: _, data } => {
                        if &data == expected_reason {
                            Ok(())
                        } else {
                            Err(format!(
                                "Error != expected error: {:?} != {:?}",
                                &data, expected_reason,
                            )
                            .into())
                        }
                    }
                    _ => {
                        tracing::error!("unexpected revert reason");
                        Err("unexpected revert reason".to_string().into())
                    }
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
            (zkevm_opcode_defs::RetOpcode::Panic, _) => {
                tracing::error!("Vm panicked it should have never happened");
                Err("expected revert but call Panicked".to_string().into())
            }
        }
    }

    /// Adds an expectCall to the tracker.
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

    fn handle_return<H: HistoryMode>(
        &mut self,
        state: &VmLocalStateData<'_>,
        data: &AfterExecutionData,
        memory: &SimpleMemory<H>,
    ) {
        // Skip check if there are no expected actions
        let Some(action) = self.next_return_action.as_mut() else { return };
        // We only care about the certain depth
        let callstack_depth = state.vm_local_state.callstack.depth();
        if callstack_depth != action.target_depth {
            return
        }

        // Skip check if opcode is not Ret
        let Opcode::Ret(op) = data.opcode.variant.opcode else { return };
        // Check how many retunrs we need to skip before finding the actual one
        if action.returns_to_skip != 0 {
            action.returns_to_skip -= 1;
            return
        }

        // The desired return opcode was found
        let ActionOnReturn::ExpectRevert {
            reason,
            depth,
            prev_exception_handler_pc: exception_handler,
            prev_continue_pc: continue_pc,
        } = &action.action;
        match op {
            RetOpcode::Revert => {
                tracing::debug!(wanted = %depth, current_depth = %callstack_depth, opcode = ?data.opcode.variant.opcode, "expectRevert");
                let (Some(exception_handler), Some(continue_pc)) =
                    (*exception_handler, *continue_pc)
                else {
                    tracing::error!("exceptRevert missing stored continuations");
                    return
                };

                self.one_time_actions.push(
                    Self::handle_except_revert(reason.as_ref(), op, state, memory)
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
                self.next_return_action = None;
            }
            RetOpcode::Ok => {
                let Some(exception_handler) = *exception_handler else {
                    tracing::error!("exceptRevert missing stored continuations");
                    return
                };
                if let Err(err) = Self::handle_except_revert(reason.as_ref(), op, state, memory) {
                    self.one_time_actions.push(FinishCycleOneTimeActions::ForceRevert {
                        error: err,
                        exception_handler,
                    });
                }
                self.next_return_action = None;
            }
            RetOpcode::Panic => (),
        }
    }
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

fn compare_logs(expected_logs: &[LogEntry], actual_logs: &[LogEntry], checks: EmitChecks) -> bool {
    let mut expected_iter = expected_logs.iter().peekable();
    let mut actual_iter = actual_logs.iter();

    while let Some(expected_log) = expected_iter.peek() {
        if let Some(actual_log) = actual_iter.next() {
            if are_logs_equal(expected_log, actual_log, &checks) {
                expected_iter.next(); // Move to the next expected log
            } else {
                return false
            }
        } else {
            // No more actual logs to compare
            return false
        }
    }

    true
}

fn are_logs_equal(a: &LogEntry, b: &LogEntry, emit_checks: &EmitChecks) -> bool {
    let address_match = match emit_checks.address {
        Some(address) => b.address == address,
        None => true,
    };

    let topics_match = emit_checks.topics.iter().enumerate().all(|(i, &check)| {
        if check {
            a.topics.get(i) == b.topics.get(i)
        } else {
            true
        }
    });

    let data_match = if emit_checks.data { a.data == b.data } else { true };

    address_match && topics_match && data_match
}
