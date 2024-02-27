use crate::{
    events::LogEntry,
    farcall::{self, FarCallHandler, MockCall, MockedCalls, ParsedFarCall},
    utils::{ToH160, ToH256, ToU256},
};
use alloy_primitives::{Address, Bytes, FixedBytes, I256 as rI256};
use alloy_sol_types::{SolInterface, SolValue};
use era_test_node::utils::bytecode_to_factory_dep;
use ethers::{
    signers::{LocalWallet, Signer},
    types::TransactionRequest,
};
use eyre::Context;
use foundry_cheatcodes::{BroadcastableTransaction, BroadcastableTransactions, CheatsConfig};
use foundry_cheatcodes_spec::Vm;
use foundry_common::{
    conversion_utils::{h160_to_address, h256_to_h160, revm_u256_to_u256, u256_to_revm_u256},
    DualCompiledContract, StorageModifications,
};
use foundry_evm_core::{
    backend::{DatabaseExt, LocalForkId},
    constants::MAGIC_ASSUME,
    era_revm::{db::RevmDatabaseForEra, storage_view::StorageView, transactions::storage_to_state},
    fork::CreateFork,
    opts::EvmOpts,
};
use itertools::Itertools;
use multivm::{
    interface::{dyn_tracers::vm_1_4_0::DynTracer, tracer::TracerExecutionStatus, VmRevertReason},
    vm_latest::{
        BootloaderState, HistoryMode, L1BatchEnv, SimpleMemory, SystemEnv, VmTracer, ZkSyncVmState,
    },
    zk_evm_1_3_3::zkevm_opcode_defs::CALL_SYSTEM_ABI_REGISTERS,
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
    primitives::{
        ruint::Uint, Account, AccountInfo, AccountStatus, BlockEnv, Bytecode, CfgEnv, CreateScheme,
        Env, HashMap as rHashMap, SpecId, KECCAK_EMPTY, U256 as rU256,
    },
    DatabaseCommit, JournaledState,
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
    sync::{Arc, MutexGuard, RwLock},
};
use zksync_basic_types::{AccountTreeId, H160, H256, U256};
use zksync_state::{ReadStorage, StoragePtr, WriteStorage};
use zksync_types::{
    block::{pack_block_info, unpack_block_info},
    get_code_key, get_nonce_key,
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
    LogQuery, StorageKey, Timestamp, ACCOUNT_CODE_STORAGE_ADDRESS, CONTRACT_DEPLOYER_ADDRESS,
    L2_ETH_TOKEN_ADDRESS, SYSTEM_CONTEXT_ADDRESS,
};
use zksync_utils::{bytecode::CompressedBytecodeInfo, h256_to_u256, u256_to_h256};

type EraDb<DB> = StorageView<RevmDatabaseForEra<DB>>;
type PcOrImm = <EncodingModeProduction as VmEncodingMode<8>>::PcOrImm;

// address(uint160(uint256(keccak256('hevm cheat code'))))
// 0x7109709ecfa91a80626ff3989d68f67f5b1dd12d
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

//same as above, except without
// CONTRACT_DEPLOYER_ADDRESS
// MSG_VALUE_SIMULATOR_ADDRESS
// and with
// CHEATCODE_ADDRESS
const BROADCAST_IGNORED_CONTRACTS: [H160; 19] = [
    zksync_types::BOOTLOADER_ADDRESS,
    zksync_types::ACCOUNT_CODE_STORAGE_ADDRESS,
    zksync_types::NONCE_HOLDER_ADDRESS,
    zksync_types::KNOWN_CODES_STORAGE_ADDRESS,
    zksync_types::IMMUTABLE_SIMULATOR_STORAGE_ADDRESS,
    zksync_types::CONTRACT_FORCE_DEPLOYER_ADDRESS,
    zksync_types::L1_MESSENGER_ADDRESS,
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
    CHEATCODE_ADDRESS,
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
    storage_modifications: StorageModifications,
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
    broadcastable_transactions: Arc<RwLock<BroadcastableTransactions>>,
    transact_logs: Vec<LogEntry>,
    mocked_calls: MockedCalls,
    farcall_handler: FarCallHandler,
    use_evm_fork: Option<Env>,
    initialized_evm_forks: HashSet<LocalForkId>,
    dual_compiled_contracts: Vec<DualCompiledContract>,
    script_wallets: Arc<RwLock<Vec<LocalWallet>>>,
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
    RollFork { block_number: Uint<256, 4>, fork_id: Option<Uint<256, 4>> },
    SelectFork { fork_id: U256 },
    RevertToSnapshot { snapshot_id: U256 },
    Snapshot,
    SetOrigin { origin: H160 },
    Transact { fork_id: Option<U256>, tx_hash: H256 },
    MakePersistentAccount { account: H160 },
    MakePersistentAccounts { accounts: Vec<H160> },
    RevokePersistentAccount { account: H160 },
    RevokePersistentAccounts { accounts: Vec<H160> },
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
    Revert {
        depth: usize,
        prev_exception_handler_pc: Option<PcOrImm>,
        reason: Vec<u8>,
    },
}

#[derive(Debug, Default, Clone)]
struct FinishCyclePermanentActions {
    start_prank: Option<StartPrankOpts>,
    broadcast: Option<BroadcastOpts>,
}

#[derive(Debug, Clone)]
struct StartPrankOpts {
    sender: H160,
    origin: Option<H160>,
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

#[derive(Debug, Clone)]
struct BroadcastOpts {
    original_origin: H160,
    original_caller: H160,
    new_origin: H160,
    depth: usize,
}

impl<S: DatabaseExt + revm::DatabaseCommit + Send, H: HistoryMode>
    DynTracer<EraDb<S>, SimpleMemory<H>> for CheatcodeTracer
{
    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: multivm::zk_evm_1_4_0::tracing::BeforeExecutionData,
        memory: &SimpleMemory<H>,
        storage: StoragePtr<EraDb<S>>,
    ) {
        self.farcall_handler.track_active_far_calls(state, data, memory, storage);

        //store the current exception handler in expect revert
        // to be used to force a revert
        if let Some(ActionOnReturn::ExpectRevert {
            prev_exception_handler_pc,
            prev_continue_pc,
            ..
        }) = self.current_return_action()
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
        if let Some(ActionOnReturn::Revert { prev_exception_handler_pc, .. }) =
            self.current_return_action()
        {
            if matches!(data.opcode.variant.opcode, Opcode::Ret(_)) {
                let current = &state.vm_local_state.callstack.current;
                tracing::debug!(?current, "storing continuations");
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

            // Reset EVM mode
            self.use_evm_fork = None;

            // reset the test state to avoid checking again
            self.reset_test_status();
        }

        // Checks returns from contracts for expectRevert cheatcode
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

        if let Opcode::FarCall(_call) = data.opcode.variant.opcode {
            let current = state.vm_local_state.callstack.current;
            let calldata = get_calldata(&state, memory);
            if let Some(return_data) = self.mocked_calls.get_matching_return_data(
                current.code_address,
                &calldata,
                U256::from(current.context_u128_value),
            ) {
                tracing::info!(
                    calldata = hex::encode(&calldata),
                    return_data = hex::encode(&return_data),
                    "mock call matched"
                );
                self.farcall_handler.set_immediate_return(return_data);
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

        if let Opcode::Ret(_) = data.opcode.variant.opcode {
            let current = state.vm_local_state.callstack.current;
            if current.code_address != CHEATCODE_ADDRESS {
                if let Some(broadcast) = &self.permanent_actions.broadcast {
                    if state.vm_local_state.callstack.depth() == broadcast.depth {
                        // when the test ends, just make sure the tx origin is set to the
                        // original one (should never get here unless .stopBroadcast wasn't called)
                        self.one_time_actions.push(FinishCycleOneTimeActions::SetOrigin {
                            origin: broadcast.original_origin,
                        });
                    }
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
                if let Some(broadcast) = self.permanent_actions.broadcast.as_ref() {
                    let prev_cs = state
                        .vm_local_state
                        .callstack
                        .inner
                        .last()
                        .expect("callstack before the current");

                    if state.vm_local_state.callstack.depth() == broadcast.depth &&
                        prev_cs.this_address == broadcast.original_caller &&
                        !BROADCAST_IGNORED_CONTRACTS.contains(&current.code_address)
                    {
                        self.one_time_actions.push(FinishCycleOneTimeActions::SetOrigin {
                            origin: broadcast.new_origin,
                        });

                        let new_origin = broadcast.new_origin;
                        let handle = &mut storage.borrow_mut();
                        let revm_db_for_era = &handle.storage_handle;
                        let rpc = revm_db_for_era.db.lock().unwrap().active_fork_url();

                        let calldata = get_calldata(&state, memory);

                        let is_deployment =
                            current.code_address == zksync_types::CONTRACT_DEPLOYER_ADDRESS;
                        let factory_deps = if is_deployment {
                            let test_contract_hash = handle.read_value(&StorageKey::new(
                                AccountTreeId::new(zksync_types::ACCOUNT_CODE_STORAGE_ADDRESS),
                                TEST_ADDRESS.into(),
                            ));

                            let mut all_bytecodes = self.get_modified_bytecodes(vec![]);
                            all_bytecodes.extend(self.storage_modifications.known_codes.clone());

                            all_bytecodes
                                .iter()
                                .filter_map(
                                    |(k, v)| {
                                        if k != &test_contract_hash {
                                            Some(v)
                                        } else {
                                            None
                                        }
                                    },
                                )
                                .cloned()
                                .collect_vec()
                        } else {
                            vec![]
                        };

                        // used to determine whether the nonce should be decreased, since
                        // zkevm updates the nonce for the _sender_ already when we run the
                        // script/test function therefore, we should fix it and obtain the nonce
                        // that resembles the one to be used on-chain
                        let is_sender_also_caller =
                            new_origin == self.config.evm_opts.sender.to_h160();
                        let nonce_offset = if is_sender_also_caller { 1 } else { 0 };

                        let nonce = Self::get_nonce(new_origin, handle)
                            .0
                            .saturating_sub(nonce_offset.into());

                        let _gas_limit = current.ergs_remaining;

                        let (value, to) = if current.code_address ==
                            zksync_types::MSG_VALUE_SIMULATOR_ADDRESS
                        {
                            //when some eth is sent to an address in zkevm the call is replaced
                            // with a call to MsgValueSimulator, which does a mimic_call later
                            // to the original destination
                            // The value is stored in the 1st system abi register, and the
                            // original address is stored in the 2nd register
                            // see: https://github.com/matter-labs/era-test-node/blob/6ee7d29e876b75506f58355218e1ea755a315d17/etc/system-contracts/contracts/MsgValueSimulator.sol#L26-L27
                            let msg_value_simulator_value_reg_idx = CALL_SYSTEM_ABI_REGISTERS.start;
                            let msg_value_simulator_address_reg_idx =
                                msg_value_simulator_value_reg_idx + 1;
                            let value = state.vm_local_state.registers
                                [msg_value_simulator_value_reg_idx as usize];

                            let address = state.vm_local_state.registers
                                [msg_value_simulator_address_reg_idx as usize];

                            let mut bytes = [0u8; 32];
                            address.value.to_big_endian(&mut bytes);
                            let address = H256::from(bytes);

                            (Some(value.value), address.into())
                        } else {
                            (None, current.code_address)
                        };

                        let tx = BroadcastableTransaction {
                            rpc,
                            factory_deps,
                            transaction:
                                ethers::types::transaction::eip2718::TypedTransaction::Legacy(
                                    TransactionRequest {
                                        from: Some(new_origin),
                                        to: Some(ethers::types::NameOrAddress::Address(to)),
                                        //TODO: set only if set manually by user in script
                                        gas: None,
                                        value,
                                        data: Some(calldata.into()),
                                        nonce: Some(nonce),
                                        ..Default::default()
                                    },
                                ),
                        };
                        tracing::debug!(?tx, "storing for broadcast");

                        self.broadcastable_transactions.write().unwrap().push_back(tx);

                        // we increase the nonce so that future calls will have the nonce
                        // increased, simulating the previous tx being executed
                        self.set_nonce(new_origin, (Some(nonce + 1 + nonce_offset), None), handle);
                    }
                }

                if self.maybe_handle_evm_far_call(state, memory, storage) {
                    return
                }

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

impl<S: DatabaseExt + revm::DatabaseCommit + Send, H: HistoryMode> VmTracer<EraDb<S>, H>
    for CheatcodeTracer
{
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
            //insert transact logs
            for log in &self.transact_logs {
                self.recorded_logs.insert(log.to_owned());
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
            let mut actual_logs = crate::events::parse_events(actual_events);
            actual_logs.extend(self.transact_logs.clone());

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
                FinishCycleOneTimeActions::MakePersistentAccount { account } => {
                    let era_db: &RevmDatabaseForEra<S> = &storage.borrow_mut().storage_handle;

                    let mut db = era_db.db.lock().unwrap();
                    db.add_persistent_account(revm::primitives::Address::from(
                        account.to_fixed_bytes(),
                    ));
                }
                FinishCycleOneTimeActions::MakePersistentAccounts { accounts } => {
                    let era_db: &RevmDatabaseForEra<S> = &storage.borrow_mut().storage_handle;

                    let mut db = era_db.db.lock().unwrap();
                    db.extend_persistent_accounts(
                        accounts
                            .into_iter()
                            .map(|a: H160| revm::primitives::Address::from(a.to_fixed_bytes()))
                            .collect::<Vec<revm::primitives::Address>>(),
                    );
                }
                FinishCycleOneTimeActions::RevokePersistentAccount { account } => {
                    let era_db: &RevmDatabaseForEra<S> = &storage.borrow_mut().storage_handle;
                    let mut db = era_db.db.lock().unwrap();
                    db.remove_persistent_account(&revm::primitives::Address::from(
                        account.to_fixed_bytes(),
                    ));
                }
                FinishCycleOneTimeActions::RevokePersistentAccounts { accounts } => {
                    let era_db: &RevmDatabaseForEra<S> = &storage.borrow_mut().storage_handle;
                    let mut db = era_db.db.lock().unwrap();
                    db.remove_persistent_accounts(
                        accounts
                            .into_iter()
                            .map(|a: H160| revm::primitives::Address::from(a.to_fixed_bytes()))
                            .collect::<Vec<revm::primitives::Address>>(),
                    );
                }
                FinishCycleOneTimeActions::StoreFactoryDep { hash, bytecode } => state
                    .decommittment_processor
                    .populate(vec![(hash, bytecode)], Timestamp(state.local_state.timestamp)),
                FinishCycleOneTimeActions::CreateSelectFork { url_or_alias, block_number } => {
                    let mut storage = storage.borrow_mut();
                    let modified_storage =
                        self.get_modified_storage(storage.modified_storage_keys());
                    let modified_bytecodes = self.get_modified_bytecodes(
                        bootloader_state.get_last_tx_compressed_bytecodes(),
                    );

                    storage.clean_cache();
                    let fork_id = {
                        let era_db: &RevmDatabaseForEra<S> = &storage.storage_handle;
                        let bytecodes = into_revm_bytecodes(modified_bytecodes.clone());
                        state.decommittment_processor.populate(
                            bytecodes
                                .clone()
                                .into_iter()
                                .filter(|(key, _)| {
                                    !state
                                        .decommittment_processor
                                        .known_bytecodes
                                        .inner()
                                        .contains_key(key)
                                })
                                .collect(),
                            Timestamp(state.local_state.timestamp),
                        );

                        let mut journaled_state = JournaledState::new(SpecId::LATEST, vec![]);
                        journaled_state.state =
                            storage_to_state(era_db, &modified_storage, bytecodes);

                        let mut db = era_db.db.lock().unwrap();
                        let era_env = self.env.get().unwrap();
                        let mut env = into_revm_env(era_env);
                        let fork_id = db
                            .create_select_fork(
                                create_fork_request(
                                    era_env,
                                    self.config.clone(),
                                    block_number,
                                    &url_or_alias,
                                ),
                                &mut env,
                                &mut journaled_state,
                            )
                            .unwrap();

                        self.maybe_switch_vm(db, fork_id, false);

                        fork_id
                    };
                    storage.modified_storage_keys = modified_storage;

                    self.return_data = Some(fork_id.to_return_data());
                }
                FinishCycleOneTimeActions::CreateFork { url_or_alias, block_number } => {
                    // TODO MULTIVM spin up the new fork in EVM
                    let era_env = self.env.get().unwrap();
                    let fork = create_fork_request(
                        era_env,
                        self.config.clone(),
                        block_number,
                        &url_or_alias,
                    );
                    let era_db = &storage.borrow_mut().storage_handle;
                    let mut db = era_db.db.lock().unwrap();
                    let fork_id = db.create_fork(fork).unwrap();

                    self.return_data = Some(fork_id.to_return_data());
                }
                FinishCycleOneTimeActions::RollFork { block_number, fork_id } => {
                    let modified_storage =
                        self.get_modified_storage(storage.borrow_mut().modified_storage_keys());
                    let modified_bytecodes = self.get_modified_bytecodes(
                        bootloader_state.get_last_tx_compressed_bytecodes(),
                    );

                    let mut storage = storage.borrow_mut();

                    storage.clean_cache();
                    {
                        let era_db = &storage.storage_handle;
                        let bytecodes = into_revm_bytecodes(modified_bytecodes.clone());
                        state.decommittment_processor.populate(
                            bytecodes
                                .clone()
                                .into_iter()
                                .filter(|(key, _)| {
                                    !state
                                        .decommittment_processor
                                        .known_bytecodes
                                        .inner()
                                        .contains_key(key)
                                })
                                .collect(),
                            Timestamp(state.local_state.timestamp),
                        );

                        let mut journaled_state = JournaledState::new(SpecId::LATEST, vec![]);
                        journaled_state.state =
                            storage_to_state(era_db, &modified_storage, bytecodes);

                        let mut db = era_db.db.lock().unwrap();
                        let era_env = self.env.get().unwrap();
                        let mut env = into_revm_env(era_env);
                        db.roll_fork(fork_id, block_number, &mut env, &mut journaled_state)
                            .unwrap();

                        if let Some(fork_id) = fork_id {
                            self.maybe_switch_vm(db, fork_id, true);
                        }
                    };
                    storage.modified_storage_keys = modified_storage;
                }
                FinishCycleOneTimeActions::SelectFork { fork_id } => {
                    // TODO MULTIVM switch to EVM
                    let revm_fork_id = rU256::from(fork_id.as_u128());

                    let mut storage = storage.borrow_mut();
                    let modified_storage =
                        self.get_modified_storage(storage.modified_storage_keys());
                    let modified_bytecodes = self.get_modified_bytecodes(
                        bootloader_state.get_last_tx_compressed_bytecodes(),
                    );
                    {
                        storage.clean_cache();
                        let era_db = &mut storage.storage_handle;
                        let bytecodes = into_revm_bytecodes(modified_bytecodes.clone());
                        state.decommittment_processor.populate(
                            bytecodes
                                .clone()
                                .into_iter()
                                .filter(|(key, _)| {
                                    !state
                                        .decommittment_processor
                                        .known_bytecodes
                                        .inner()
                                        .contains_key(key)
                                })
                                .collect(),
                            Timestamp(state.local_state.timestamp),
                        );

                        let mut journaled_state = JournaledState::new(SpecId::LATEST, vec![]);
                        journaled_state.state =
                            storage_to_state(era_db, &modified_storage, bytecodes);

                        let mut db = era_db.db.lock().unwrap();
                        let era_env = self.env.get().unwrap();
                        let mut env = into_revm_env(era_env);
                        db.select_fork(
                            rU256::from(fork_id.as_u128()),
                            &mut env,
                            &mut journaled_state,
                        )
                        .unwrap();

                        self.maybe_switch_vm(db, revm_fork_id, false);
                    }
                    storage.modified_storage_keys = modified_storage;

                    self.return_data = Some(vec![fork_id]);
                }
                FinishCycleOneTimeActions::RevertToSnapshot { snapshot_id } => {
                    let mut storage = storage.borrow_mut();
                    let modified_storage =
                        self.get_modified_storage(storage.modified_storage_keys());
                    storage.clean_cache();

                    {
                        let era_db = &storage.storage_handle;
                        let bytecodes = bootloader_state
                            .get_last_tx_compressed_bytecodes()
                            .iter()
                            .map(|b| bytecode_to_factory_dep(b.original.clone()))
                            .collect();

                        let mut journaled_state = JournaledState::new(SpecId::LATEST, vec![]);
                        journaled_state.state =
                            storage_to_state(era_db, &modified_storage, bytecodes);

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
                    let modified_storage =
                        self.get_modified_storage(storage.modified_storage_keys());

                    storage.clean_cache();

                    let snapshot_id = {
                        let era_db = &storage.storage_handle;
                        let bytecodes = bootloader_state
                            .get_last_tx_compressed_bytecodes()
                            .iter()
                            .map(|b| bytecode_to_factory_dep(b.original.clone()))
                            .collect();

                        let mut journaled_state = JournaledState::new(SpecId::LATEST, vec![]);
                        journaled_state.state =
                            storage_to_state(era_db, &modified_storage, bytecodes);

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
                    self.return_data = Some(snapshot_id.to_return_data());
                }
                FinishCycleOneTimeActions::ForceReturn { data, continue_pc: pc } => {
                    tracing::debug!(?data, pc, "Forcing return");
                    self.return_data = Some(data.to_return_data());
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
                    tracing::debug!(?error, pc, "Forcing revert");

                    self.return_data = Some(error.to_return_data());

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
                FinishCycleOneTimeActions::SetOrigin { origin } => {
                    let prev = state
                        .local_state
                        .callstack
                        .inner
                        .last_mut()
                        .expect("callstack before the current");
                    prev.this_address = origin;

                    let key = StorageKey::new(
                        AccountTreeId::new(zksync_types::SYSTEM_CONTEXT_ADDRESS),
                        zksync_types::SYSTEM_CONTEXT_TX_ORIGIN_POSITION,
                    );

                    storage.borrow_mut().set_value(key, origin.into());
                }
                FinishCycleOneTimeActions::Transact { fork_id, tx_hash } => {
                    let journaled_state = {
                        let bytecodes = bootloader_state
                            .get_last_tx_compressed_bytecodes()
                            .iter()
                            .map(|b| bytecode_to_factory_dep(b.original.clone()))
                            .collect();

                        let storage = storage.borrow_mut();
                        let modified_storage =
                            self.get_modified_storage(storage.modified_storage_keys());

                        let era_db = &storage.storage_handle;

                        let mut journaled_state = JournaledState::new(SpecId::LATEST, vec![]);
                        journaled_state.state =
                            storage_to_state(era_db, &modified_storage, bytecodes);

                        let mut db = era_db.db.lock().unwrap();
                        let era_env = self.env.get().unwrap();
                        let mut env = into_revm_env(era_env);

                        db.transact(
                            fork_id.map(|id| {
                                let mut arr = [0; 32];
                                id.to_big_endian(&mut arr);
                                Uint::from_be_bytes(arr)
                            }),
                            tx_hash.to_fixed_bytes().into(),
                            &mut env,
                            &mut journaled_state,
                            &mut revm::inspectors::NoOpInspector,
                        )
                        .unwrap();

                        journaled_state
                    };

                    storage.borrow_mut().read_storage_keys = Default::default();

                    for log in journaled_state.logs {
                        self.transact_logs.push(LogEntry {
                            address: log.address.to_h160(),
                            data: log.data.to_vec(),
                            topics: log
                                .topics
                                .iter()
                                .map(|b| H256::from_slice(b.as_slice()))
                                .collect(),
                        })
                    }
                }
            }
        }

        self.farcall_handler.maybe_return_early(state, bootloader_state, storage);

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
    pub fn new(
        cheatcodes_config: Arc<CheatsConfig>,
        storage_modifications: StorageModifications,
        broadcastable_transactions: Arc<RwLock<BroadcastableTransactions>>,
        dual_compiled_contracts: Vec<DualCompiledContract>,
        script_wallets: Arc<RwLock<Vec<LocalWallet>>>,
    ) -> Self {
        Self {
            config: cheatcodes_config,
            storage_modifications,
            broadcastable_transactions,
            dual_compiled_contracts,
            script_wallets,
            ..Default::default()
        }
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
                    }
                }
            }
            _ => (),
        }

        &self.test_status
    }

    pub fn dispatch_cheatcode<S: DatabaseExt + DatabaseCommit + Send, H: HistoryMode>(
        &mut self,
        state: VmLocalStateData<'_>,
        _data: AfterExecutionData,
        _memory: &SimpleMemory<H>,
        storage: StoragePtr<EraDb<S>>,
        call: Vm::VmCalls,
    ) {
        use Vm::{VmCalls::*, *};

        match call {
            activeFork(activeForkCall {}) => {
                tracing::info!("👷 Getting active fork");
                let handle = &storage.borrow_mut().storage_handle;
                let fork_id = handle.db.lock().unwrap().active_fork_id();
                assert!(fork_id.is_some(), "No active fork found. Please create a fork first.");
                self.return_data = Some(fork_id.unwrap().to_return_data());
            }
            addr(addrCall { privateKey: private_key }) => {
                tracing::info!("👷 Getting address for private key");
                let Ok(address) = zksync_types::PackedEthSignature::address_from_private_key(
                    &private_key.to_h256(),
                ) else {
                    tracing::error!("Failed generating address for private key");
                    return
                };
                self.return_data = Some(h160_to_address(address).to_return_data());
            }
            assume(assumeCall { condition }) => {
                tracing::info!(condition, "👷 Skipping fuzz test run if condition is not met");
                if condition {
                    return
                }
                self.add_revert_test(MAGIC_ASSUME.to_vec(), state.vm_local_state.callstack.depth());
            }
            deal(dealCall { account, newBalance: new_balance }) => {
                tracing::info!("👷 Setting balance for {account:?} to {new_balance}");
                if self.use_evm_fork.is_some() {
                    let era_db = &storage.borrow_mut().storage_handle;
                    let mut db = era_db.db.lock().unwrap();
                    let info = db
                        .basic(account)
                        .expect("failed getting account info")
                        .expect("expected some account info");

                    // code_hash is computed on commit
                    let mut changes = rHashMap::new();
                    changes.insert(
                        account,
                        Account {
                            info: AccountInfo {
                                balance: new_balance,
                                nonce: info.nonce,
                                code_hash: KECCAK_EMPTY,
                                code: None,
                            },
                            storage: Default::default(),
                            status: AccountStatus::Touched,
                        },
                    );
                    db.commit(changes);
                } else {
                    self.write_storage(
                        storage_key_for_eth_balance(&account.to_h160()),
                        new_balance.to_h256(),
                        &mut storage.borrow_mut(),
                    );
                }
            }
            etch(etchCall { target, newRuntimeBytecode: new_runtime_bytecode }) => {
                tracing::info!("👷 Setting address code for {target:?}");
                if self.use_evm_fork.is_some() {
                    let era_db = &storage.borrow_mut().storage_handle;
                    let mut db = era_db.db.lock().unwrap();
                    let info = db
                        .basic(target)
                        .expect("failed getting account info")
                        .expect("expected some account info");

                    // code_hash is computed on commit
                    let mut changes = rHashMap::new();
                    changes.insert(
                        target,
                        Account {
                            info: AccountInfo {
                                balance: info.balance,
                                nonce: info.nonce,
                                code_hash: KECCAK_EMPTY,
                                code: Some(Bytecode::new_raw(Bytes::from(new_runtime_bytecode))),
                            },
                            storage: Default::default(),
                            status: AccountStatus::Touched,
                        },
                    );
                    db.commit(changes);
                } else {
                    let code_key = get_code_key(&target.to_h160());
                    let (hash, code) = bytecode_to_factory_dep(new_runtime_bytecode);
                    self.store_factory_dep(hash, code);
                    self.write_storage(code_key, u256_to_h256(hash), &mut storage.borrow_mut());
                }
            }
            envAddress_0(envAddress_0Call { name }) => {
                tracing::info!("👷 Getting address env variable {name}");
                let env_var: Address =
                    parse_env(&name, |var| var.parse()).expect("Env var not address");
                self.return_data = Some(env_var.to_return_data());
            }
            envAddress_1(envAddress_1Call { name, delim }) => {
                tracing::info!("👷 Getting address env variable {name} with delimiter {delim}");
                let env_vars: Vec<Address> =
                    parse_env_array(&name, &delim, |var| var.parse()).expect("Env var not address");
                self.return_data = Some(env_vars.to_return_data());
            }
            envBool_0(envBool_0Call { name }) => {
                tracing::info!("👷 Getting bool env variable {name}");
                let env_var: bool = parse_env(&name, |var| var.parse()).expect("Env var not bool");
                self.return_data = Some(env_var.to_return_data());
            }
            envBool_1(envBool_1Call { name, delim }) => {
                tracing::info!("👷 Getting bool env variable {name} with delimiter {delim}");
                let env_vars: Vec<bool> =
                    parse_env_array(&name, &delim, |var| var.parse()).expect("Env var not bool");
                self.return_data = Some(env_vars.to_return_data());
            }
            envBytes_0(envBytes_0Call { name }) => {
                tracing::info!("👷 Getting bytes env variable {name}");
                let env_var: Bytes =
                    parse_env(&name, |var| var.parse()).expect("Env var not bytes");
                self.return_data = Some(env_var.to_return_data());
            }
            envBytes_1(envBytes_1Call { name, delim }) => {
                tracing::info!("👷 Getting bytes env variable {name} with delimiter {delim}");
                let env_vars: Vec<Bytes> =
                    parse_env_array(&name, &delim, |var| var.parse()).expect("Env var not bytes");
                self.return_data = Some(env_vars.to_return_data());
            }
            envBytes32_0(envBytes32_0Call { name }) => {
                tracing::info!("👷 Getting bytes32 env variable {name}");
                let env_var: FixedBytes<32> =
                    parse_env(&name, |var| var.parse()).expect("Env var not bytes32");
                self.return_data = Some(env_var.to_return_data());
            }
            envBytes32_1(envBytes32_1Call { name, delim }) => {
                tracing::info!("👷 Getting bytes32 env variable {name} with delimiter {delim}");
                let env_vars: Vec<FixedBytes<32>> =
                    parse_env_array(&name, &delim, |var| var.parse()).expect("Env var not bytes32");
                self.return_data = Some(env_vars.to_return_data());
            }
            envInt_0(envInt_0Call { name }) => {
                tracing::info!("👷 Getting int256 env variable {name}");
                let env_var: rI256 = parse_env(&name, |var| var.parse()).expect("Env var not int");
                self.return_data = Some(env_var.to_return_data());
            }
            envInt_1(envInt_1Call { name, delim }) => {
                tracing::info!("👷 Getting int256 env variable {name} with delimiter {delim}");
                let env_vars: Vec<rI256> =
                    parse_env_array(&name, &delim, |var| var.parse()).expect("Env var not int");
                self.return_data = Some(env_vars.to_return_data());
            }
            envString_0(envString_0Call { name }) => {
                tracing::info!("👷 Getting string env variable {name}");
                let env_var: String =
                    parse_env(&name, |var| var.parse()).expect("Env var not string");
                self.return_data = Some(env_var.to_return_data());
            }
            envString_1(envString_1Call { name, delim }) => {
                tracing::info!("👷 Getting string env variable {name} with delimiter {delim}");
                let env_vars: Vec<String> =
                    parse_env_array(&name, &delim, |var| var.parse()).expect("Env var not string");
                self.return_data = Some(env_vars.to_return_data());
            }
            envUint_0(envUint_0Call { name }) => {
                tracing::info!("👷 Getting uint256 env variable {name}");
                let env_var: rU256 = parse_env(&name, |var| var.parse()).expect("Env var not int");
                self.return_data = Some(env_var.to_return_data());
            }
            envUint_1(envUint_1Call { name, delim }) => {
                tracing::info!("👷 Getting uint256 env variable {name} with delimiter {delim}");
                let env_vars: Vec<rU256> =
                    parse_env_array(&name, &delim, |var| var.parse()).expect("Env var not int");
                self.return_data = Some(env_vars.to_return_data());
            }
            envOr_0(envOr_0Call { name, defaultValue }) => {
                tracing::info!("👷 Getting bool env variable {name} with fallback {defaultValue}");
                let env_var: bool = parse_env(&name, |var| var.parse()).unwrap_or(defaultValue);
                self.return_data = Some(env_var.to_return_data());
            }
            envOr_1(envOr_1Call { name, defaultValue }) => {
                tracing::info!(
                    "👷 Getting uint256 env variable {name} with fallback {defaultValue}"
                );
                let env_var: rU256 = parse_env(&name, |var| var.parse()).unwrap_or(defaultValue);
                self.return_data = Some(env_var.to_return_data());
            }
            envOr_2(envOr_2Call { name, defaultValue }) => {
                tracing::info!(
                    "👷 Getting int256 env variable {name} with fallback {defaultValue}"
                );
                let env_var: rI256 = parse_env(&name, |var| var.parse()).unwrap_or(defaultValue);
                self.return_data = Some(env_var.to_return_data());
            }
            envOr_3(envOr_3Call { name, defaultValue }) => {
                tracing::info!(
                    "👷 Getting address env variable {name} with fallback {defaultValue}"
                );
                let env_var: Address = parse_env(&name, |var| var.parse()).unwrap_or(defaultValue);
                self.return_data = Some(env_var.to_return_data());
            }
            envOr_4(envOr_4Call { name, defaultValue }) => {
                tracing::info!(
                    "👷 Getting bytes32 env variable {name} with fallback {defaultValue}"
                );
                let env_var: FixedBytes<32> =
                    parse_env(&name, |var| var.parse()).unwrap_or(defaultValue);
                self.return_data = Some(env_var.to_return_data());
            }
            envOr_5(envOr_5Call { name, defaultValue }) => {
                tracing::info!(
                    "👷 Getting string env variable {name} with fallback {defaultValue}"
                );
                let env_var = parse_env(&name, |var| {
                    Ok::<_, &(dyn std::error::Error + Send + Sync)>(var.to_string())
                })
                .unwrap_or(defaultValue);
                self.return_data = Some(env_var.to_return_data());
            }
            envOr_6(envOr_6Call { name, defaultValue }) => {
                tracing::info!(
                    "👷 Getting bytes env variable {name} with fallback {defaultValue:?}"
                );
                let env_var: Bytes =
                    parse_env(&name, |var| var.parse()).unwrap_or(defaultValue.into());
                self.return_data = Some(env_var.to_return_data());
            }
            envOr_7(envOr_7Call { name, delim, defaultValue }) => {
                tracing::info!("👷 Getting bool env variable {name} with delimiter {delim} and fallback {defaultValue:?}");
                let env_vars: Vec<bool> =
                    parse_env_array(&name, &delim, |var| var.parse()).unwrap_or(defaultValue);
                self.return_data = Some(env_vars.to_return_data());
            }
            envOr_8(envOr_8Call { name, delim, defaultValue }) => {
                tracing::info!("👷 Getting uint256 env variable {name} with delimiter {delim} and fallback {defaultValue:?}");
                let env_vars: Vec<rU256> =
                    parse_env_array(&name, &delim, |var| var.parse()).unwrap_or(defaultValue);
                self.return_data = Some(env_vars.to_return_data());
            }
            envOr_9(envOr_9Call { name, delim, defaultValue }) => {
                tracing::info!("👷 Getting int256 env variable {name} with delimiter {delim} and fallback {defaultValue:?}");
                let env_vars: Vec<rI256> =
                    parse_env_array(&name, &delim, |var| var.parse()).unwrap_or(defaultValue);
                self.return_data = Some(env_vars.to_return_data());
            }
            envOr_10(envOr_10Call { name, delim, defaultValue }) => {
                tracing::info!("👷 Getting address env variable {name} with delimiter {delim} and fallback {defaultValue:?}");
                let env_vars: Vec<Address> =
                    parse_env_array(&name, &delim, |var| var.parse()).unwrap_or(defaultValue);
                self.return_data = Some(env_vars.to_return_data());
            }
            envOr_11(envOr_11Call { name, delim, defaultValue }) => {
                tracing::info!("👷 Getting bytes32 env variable {name} with delimiter {delim} and fallback {defaultValue:?}");
                let env_vars: Vec<FixedBytes<32>> =
                    parse_env_array(&name, &delim, |var| var.parse()).unwrap_or(defaultValue);
                self.return_data = Some(env_vars.to_return_data());
            }
            envOr_12(envOr_12Call { name, delim, defaultValue }) => {
                tracing::info!("👷 Getting string env variable {name} with delimiter {delim} and fallback {defaultValue:?}");
                let env_vars: Vec<String> =
                    parse_env_array(&name, &delim, |var| var.parse()).unwrap_or(defaultValue);
                self.return_data = Some(env_vars.to_return_data());
            }
            envOr_13(envOr_13Call { name, delim, defaultValue }) => {
                tracing::info!("👷 Getting bytes env variable {name} with delimiter {delim} and fallback {defaultValue:?}");
                let env_vars: Vec<Bytes> = parse_env_array(&name, &delim, |var| var.parse())
                    .unwrap_or(defaultValue.into_iter().map(|v| v.into()).collect());
                self.return_data = Some(env_vars.to_return_data());
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

                self.return_data = Some(encoded_stdout.to_return_data());
            }
            getNonce_0(getNonce_0Call { account }) => {
                tracing::info!("👷 Getting nonce for {account:?}");
                let account_nonce = if self.use_evm_fork.is_some() {
                    let era_db = &storage.borrow_mut().storage_handle;
                    let mut db = era_db.db.lock().unwrap();
                    let info = db.basic(account).ok().flatten().expect("account info not found");
                    U256::from(info.nonce)
                } else {
                    let (account_nonce, _) =
                        Self::get_nonce(account.to_h160(), &mut storage.borrow_mut());
                    tracing::info!(
                        "👷 Nonces for account {:?} are {}",
                        account,
                        account_nonce.as_u64()
                    );
                    account_nonce
                };
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
                    .filter(|log| !INTERNAL_CONTRACT_ADDRESSES.contains(&log.address))
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

                self.return_data = Some(logs.to_return_data());

                //clean up logs
                self.recorded_logs = HashSet::new();
                //disable flag of recording logs
                self.recording_logs = false;
            }
            isPersistent(isPersistentCall { account }) => {
                tracing::info!("👷 Checking if account {:?} is persistent", account);
                let era_db: &RevmDatabaseForEra<S> = &storage.borrow_mut().storage_handle;
                let db = era_db.db.lock().unwrap();
                let is_persistent = db.is_persistent(&revm::primitives::Address::from(
                    account.to_h160().to_fixed_bytes(),
                ));
                self.return_data = Some(is_persistent.to_return_data());
            }
            load(loadCall { target, slot }) => {
                if H160(target.0 .0) != CHEATCODE_ADDRESS {
                    tracing::info!("👷 Getting storage slot {:?} for account {:?}", slot, target);
                    let key = StorageKey::new(AccountTreeId::new(target.to_h160()), H256(*slot));
                    let mut storage = storage.borrow_mut();
                    let value = storage.read_value(&key);
                    self.return_data = Some(vec![h256_to_u256(value)]);
                } else {
                    self.return_data = Some(vec![U256::zero()]);
                }
            }
            makePersistent_0(makePersistent_0Call { account }) => {
                tracing::info!("👷 Making account {:?} persistent", account);
                self.one_time_actions.push(FinishCycleOneTimeActions::MakePersistentAccount {
                    account: account.to_h160(),
                });
            }
            makePersistent_1(makePersistent_1Call { account0, account1 }) => {
                tracing::info!("👷 Making accounts {:?} and {:?} persistent", account0, account1);
                self.one_time_actions.push(FinishCycleOneTimeActions::MakePersistentAccounts {
                    accounts: vec![account0.to_h160(), account1.to_h160()],
                });
            }
            makePersistent_2(makePersistent_2Call { account0, account1, account2 }) => {
                tracing::info!(
                    "👷 Making accounts {:?}, {:?} and {:?} persistent",
                    account0,
                    account1,
                    account2
                );
                self.one_time_actions.push(FinishCycleOneTimeActions::MakePersistentAccounts {
                    accounts: vec![account0.to_h160(), account1.to_h160(), account2.to_h160()],
                });
            }
            makePersistent_3(makePersistent_3Call { accounts }) => {
                tracing::info!("👷 Making accounts {:?} persistent", accounts);
                self.one_time_actions.push(FinishCycleOneTimeActions::MakePersistentAccounts {
                    accounts: accounts.into_iter().map(|a| a.to_h160()).collect(),
                });
            }
            mockCall_0(mockCall_0Call { callee, data, returnData }) => {
                tracing::info!("👷 Mocking call to {callee:?}");
                self.mocked_calls.insert(
                    MockCall { address: callee.to_h160(), value: None, calldata: data },
                    returnData,
                )
            }
            mockCall_1(mockCall_1Call { callee, msgValue, data, returnData }) => {
                tracing::info!("👷 Mocking call to {callee:?}");
                self.mocked_calls.insert(
                    MockCall {
                        address: callee.to_h160(),
                        value: Some(revm_u256_to_u256(msgValue)),
                        calldata: data,
                    },
                    returnData,
                )
            }
            clearMockedCalls(clearMockedCallsCall {}) => {
                tracing::info!("👷 Clearing all mocked calls");
                self.mocked_calls.clear();
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
                self.return_data = Some(data.to_return_data());
            }
            revertTo(revertToCall { snapshotId }) => {
                tracing::info!("👷 Reverting to snapshot {}", snapshotId);
                self.one_time_actions.push(FinishCycleOneTimeActions::RevertToSnapshot {
                    snapshot_id: snapshotId.to_u256(),
                });
                self.return_data = Some(true.to_return_data());
            }
            revokePersistent_0(revokePersistent_0Call { account }) => {
                tracing::info!("👷 Revoking persistence for account {:?}", account);
                self.one_time_actions.push(FinishCycleOneTimeActions::RevokePersistentAccount {
                    account: account.to_h160(),
                });
            }
            revokePersistent_1(revokePersistent_1Call { accounts }) => {
                tracing::info!("👷 Revoking persistence for accounts {:?}", accounts);
                self.one_time_actions.push(FinishCycleOneTimeActions::RevokePersistentAccounts {
                    accounts: accounts.into_iter().map(|a| a.to_h160()).collect(),
                });
            }
            roll(rollCall { newHeight: new_height }) => {
                tracing::info!("👷 Setting block number to {}", new_height);
                if let Some(env) = self.use_evm_fork.as_mut() {
                    env.block.number = new_height;
                } else {
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
            }
            rollFork_0(rollFork_0Call { blockNumber }) => {
                tracing::info!("👷 Rolling active fork to block number {}", blockNumber);
                self.one_time_actions.push(FinishCycleOneTimeActions::RollFork {
                    block_number: blockNumber,
                    fork_id: None,
                });
            }
            rollFork_2(rollFork_2Call { blockNumber, forkId }) => {
                tracing::info!("👷 Rolling fork {} to block number {}", forkId, blockNumber);
                self.one_time_actions.push(FinishCycleOneTimeActions::RollFork {
                    block_number: blockNumber,
                    fork_id: Some(forkId),
                });
            }
            rpcUrl(rpcUrlCall { rpcAlias }) => {
                tracing::info!("👷 Getting rpc url of {}", rpcAlias);
                let rpc_endpoints = &self.config.rpc_endpoints;
                let rpc_url = match rpc_endpoints.get(&rpcAlias) {
                    Some(Ok(url)) => url.clone(),
                    _ => {
                        //this should revert but we don't have reverts yet
                        panic!(
                            "Failed to resolve env var `{}`: environment variable not found",
                            rpcAlias
                        )
                    }
                };

                self.return_data = Some(rpc_url.to_return_data());
            }
            rpcUrls(rpcUrlsCall {}) => {
                tracing::info!("👷 Getting rpc urls");
                let rpc_endpoints = &self.config.rpc_endpoints;
                let urls = rpc_endpoints
                    .iter()
                    .map(|(alias, url)| Rpc {
                        key: alias.clone(),
                        url: url
                            .as_ref()
                            .map(|value| value.clone())
                            .unwrap_or_else(|_| alias.clone()),
                    })
                    .collect_vec();

                self.return_data = Some(urls.to_return_data());
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

                let address_with_checksum = value.to_checksum(None);
                self.return_data = Some(address_with_checksum.to_return_data());
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
                self.return_data = Some(bool_value.to_return_data());
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
                self.return_data = Some(uint_value.to_return_data());
            }
            setEnv(setEnvCall { name, value }) => {
                tracing::info!("👷 Setting env variable {name:?} to {value:?}");
                std::env::set_var(name, value);
            }
            setNonce(setNonceCall { account, newNonce: new_nonce }) => {
                tracing::info!("👷 Setting nonce for {account:?} to {new_nonce}");
                if self.use_evm_fork.is_some() {
                    let era_db = &storage.borrow_mut().storage_handle;
                    let mut db = era_db.db.lock().unwrap();
                    let info = db
                        .basic(account)
                        .expect("failed getting account info")
                        .expect("expected some account info");

                    // code_hash is computed on commit
                    let mut changes = rHashMap::new();
                    changes.insert(
                        account,
                        Account {
                            info: AccountInfo {
                                balance: info.balance,
                                nonce: new_nonce,
                                code_hash: KECCAK_EMPTY,
                                code: None,
                            },
                            storage: Default::default(),
                            status: AccountStatus::Touched,
                        },
                    );
                    db.commit(changes);
                } else {
                    if self
                        .set_nonce(
                            account.to_h160(),
                            (Some(new_nonce.into()), Some(new_nonce.into())),
                            &mut storage.borrow_mut(),
                        )
                        .is_none()
                    {
                        tracing::error!("👷 Setting nonces failed");
                        return
                    }
                };

                tracing::info!("👷 Nonce for account {:?} has been set to {}", account, new_nonce);
            }
            sign_0(sign_0Call { privateKey: private_key, digest }) => {
                tracing::info!("👷 Signing digest with private key");
                let Ok(signature) = zksync_types::PackedEthSignature::sign(
                    &private_key.to_h256(),
                    digest.as_slice(),
                ) else {
                    tracing::error!("Failed to sign digest with private key");
                    return
                };

                let r = signature.r();
                let s = signature.s();
                // Ethereum signed message produced by most clients contains v where v = 27 +
                // recovery_id(0,1,2,3), but for some clients v = recovery_id(0,1,2,3). The library
                // that we use for signature verification (written for bitcoin)
                // expects v = recovery_id and to able to recover the address from Solidity it
                // expects v = 27 + recovery_id.
                let v = signature.v() + 27;

                self.return_data = Some(vec![v.into(), r.into(), s.into()])
            }
            snapshot(snapshotCall {}) => {
                tracing::info!("👷 Creating snapshot");
                self.one_time_actions.push(FinishCycleOneTimeActions::Snapshot);
            }
            startBroadcast_0(startBroadcast_0Call {}) => {
                tracing::info!("👷 Starting broadcast with default origin");

                self.start_broadcast(&storage, &state, None)
            }
            startBroadcast_1(startBroadcast_1Call { signer }) => {
                let origin = signer.to_h160();
                tracing::info!("👷 Starting broadcast with given origin: {origin}");
                self.start_broadcast(&storage, &state, Some(origin))
            }
            startBroadcast_2(startBroadcast_2Call { privateKey }) => {
                let chain_id = self.env.get().unwrap().system_env.chain_id.as_u64();
                let Some(wallet) =
                    crate::utils::parse_wallet(&privateKey).map(|w| w.with_chain_id(chain_id))
                else {
                    tracing::error!(cheatcode = "startBroadcast", "unable to parse private key");
                    return
                };

                let origin = wallet.address();
                tracing::info!("👷 Starting broadcast with origin from private key: {origin}");
                self.script_wallets.write().unwrap().push(wallet);
                self.start_broadcast(&storage, &state, Some(origin))
            }
            startPrank_0(startPrank_0Call { msgSender: msg_sender }) => {
                tracing::info!("👷 Starting prank to {msg_sender:?}");
                self.start_prank(&storage, msg_sender.to_h160(), None);
            }
            startPrank_1(startPrank_1Call { msgSender: msg_sender, txOrigin: tx_origin }) => {
                tracing::info!("👷 Starting prank to {msg_sender:?} with origin {tx_origin:?}");
                self.start_prank(&storage, msg_sender.to_h160(), Some(tx_origin.to_h160()))
            }
            stopBroadcast(stopBroadcastCall {}) => {
                tracing::info!("👷 Stopping broadcast");
                self.stop_broadcast();
            }
            stopPrank(stopPrankCall {}) => {
                tracing::info!("👷 Stopping prank");
                self.stop_prank(&storage);
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
                let address_with_checksum = value.to_checksum(None);
                self.return_data = Some(address_with_checksum.to_return_data());
            }
            toString_1(toString_1Call { value }) => {
                tracing::info!("Converting bytes into string");
                let bytes_value = hex::encode_prefixed(value);
                self.return_data = Some(bytes_value.to_return_data());
            }
            toString_2(toString_2Call { value }) => {
                tracing::info!("Converting bytes32 into string");
                let bytes_value = hex::encode_prefixed(value);
                self.return_data = Some(bytes_value.to_return_data());
            }
            toString_3(toString_3Call { value }) => {
                tracing::info!("Converting bool into string");
                let bool_value = value.to_string();
                self.return_data = Some(bool_value.to_return_data());
            }
            toString_4(toString_4Call { value }) => {
                tracing::info!("Converting uint256 into string");
                let uint_value = value.to_string();
                self.return_data = Some(uint_value.to_return_data());
            }
            toString_5(toString_5Call { value }) => {
                tracing::info!("Converting int256 into string");
                let int_value = value.to_string();
                self.return_data = Some(int_value.to_return_data());
            }
            transact_0(transact_0Call { txHash }) => {
                tracing::info!("👷 Transacting current fork with: {txHash:x?}");
                self.one_time_actions.push(FinishCycleOneTimeActions::Transact {
                    fork_id: None,
                    tx_hash: H256::from(txHash.0),
                });
            }
            transact_1(transact_1Call { forkId, txHash }) => {
                tracing::info!("👷 Transacting fork {forkId} with: {txHash:x?}");
                self.one_time_actions.push(FinishCycleOneTimeActions::Transact {
                    fork_id: Some(forkId.to_u256()),
                    tx_hash: H256::from(txHash.0),
                });
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
                self.return_data = Some(ffi_result.to_return_data());
            }
            warp(warpCall { newTimestamp: new_timestamp }) => {
                tracing::info!("👷 Setting block timestamp {}", new_timestamp);

                if let Some(env) = self.use_evm_fork.as_mut() {
                    env.block.timestamp = new_timestamp;
                } else {
                    let key = StorageKey::new(
                        AccountTreeId::new(zksync_types::SYSTEM_CONTEXT_ADDRESS),
                        zksync_types::CURRENT_VIRTUAL_BLOCK_INFO_POSITION,
                    );
                    let mut storage = storage.borrow_mut();
                    let (block_number, _) =
                        unpack_block_info(h256_to_u256(storage.read_value(&key)));
                    self.write_storage(
                        key,
                        u256_to_h256(pack_block_info(block_number, new_timestamp.as_limbs()[0])),
                        &mut storage,
                    );
                }
            }
            createSelectFork_0(createSelectFork_0Call { urlOrAlias }) => {
                tracing::info!("👷 Creating and selecting fork {}", urlOrAlias,);

                if self.permanent_actions.broadcast.is_none() {
                    self.one_time_actions.push(FinishCycleOneTimeActions::CreateSelectFork {
                        url_or_alias: urlOrAlias,
                        block_number: None,
                    });
                } else {
                    tracing::error!("cannot select fork during a broadcast")
                }
            }
            createSelectFork_1(createSelectFork_1Call { urlOrAlias, blockNumber }) => {
                let block_number = blockNumber.to_u256().as_u64();
                tracing::info!(
                    "👷 Creating and selecting fork {} for block number {}",
                    urlOrAlias,
                    block_number
                );

                if self.permanent_actions.broadcast.is_none() {
                    self.one_time_actions.push(FinishCycleOneTimeActions::CreateSelectFork {
                        url_or_alias: urlOrAlias,
                        block_number: Some(block_number),
                    });
                } else {
                    tracing::error!("cannot select fork during a broadcast")
                }
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

                if self.permanent_actions.broadcast.is_none() {
                    self.one_time_actions
                        .push(FinishCycleOneTimeActions::SelectFork { fork_id: forkId.to_u256() });
                } else {
                    tracing::error!("cannot select fork during broadcast")
                }
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
            code => {
                tracing::error!("👷 Unrecognized cheatcode {:?}", code);
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

    /// Returns a given account's nonce
    ///
    /// The first item of the tuple represents the total number of transactions,
    /// meanwhile the second represents the number of contract deployed
    fn get_nonce<S: ReadStorage>(account: H160, storage: &mut RefMut<S>) -> (U256, U256) {
        let key = get_nonce_key(&account);
        let full_nonce = storage.read_value(&key);

        decompose_full_nonce(h256_to_u256(full_nonce))
    }

    /// Sets a given account's nonces
    ///
    /// Returns the new nonce
    fn set_nonce<S: WriteStorage>(
        &mut self,
        account: H160,
        (tx_nonce, deploy_nonce): (Option<U256>, Option<U256>),
        storage: &mut RefMut<S>,
    ) -> Option<(U256, U256)> {
        let key = get_nonce_key(&account);
        let (mut account_nonce, mut deployment_nonce) = Self::get_nonce(account, storage);
        if let Some(tx_nonce) = tx_nonce {
            if account_nonce >= tx_nonce {
                tracing::error!(?account, value = ?account_nonce, requested = ?tx_nonce, "account nonce is already set to a higher value");
                return None
            }

            account_nonce = tx_nonce;
        }

        if let Some(deploy_nonce) = deploy_nonce {
            if deployment_nonce >= deploy_nonce {
                tracing::error!(?account, value = ?deployment_nonce, requested = ?deploy_nonce, "deployment nonce is already set to a higher value");
                return None
            }

            deployment_nonce = deploy_nonce;
        }

        let new_full_nonce = nonces_to_full_nonce(account_nonce, deployment_nonce);
        self.write_storage(key, u256_to_h256(new_full_nonce), storage);

        Some((account_nonce, deployment_nonce))
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

    fn current_return_action(&mut self) -> Option<&mut ActionOnReturn> {
        self.next_return_action.as_mut().map(|action| &mut action.action)
    }

    fn add_expect_revert(&mut self, reason: Option<Vec<u8>>, depth: usize) {
        if self.current_return_action().is_some() {
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

    fn add_revert_test(&mut self, reason: Vec<u8>, depth: usize) {
        if self.current_return_action().is_some() {
            panic!("revert test already set")
        }

        //-1: Because we are working with return opcode and it pops the stack after execution
        let action =
            ActionOnReturn::Revert { depth: depth - 1, prev_exception_handler_pc: None, reason };

        self.next_return_action =
            Some(NextReturnAction { target_depth: depth - 1, action, returns_to_skip: 0 });
    }

    fn handle_expect_revert<H: HistoryMode>(
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

        // The desired return opcode was found
        match &action.action {
            ActionOnReturn::ExpectRevert {
                reason,
                depth,
                prev_exception_handler_pc: exception_handler,
                prev_continue_pc: continue_pc,
            } => {
                // Check how many returns we need to skip before finding the actual one
                if action.returns_to_skip != 0 {
                    action.returns_to_skip -= 1;
                    return
                }

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
                            Self::handle_expect_revert(reason.as_ref(), op, state, memory)
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
                        if let Err(error) =
                            Self::handle_expect_revert(reason.as_ref(), op, state, memory)
                        {
                            self.one_time_actions.push(FinishCycleOneTimeActions::ForceRevert {
                                error,
                                exception_handler,
                            });
                        }
                        self.next_return_action = None;
                    }
                    RetOpcode::Panic => (),
                }
            }
            ActionOnReturn::Revert { depth, reason, prev_exception_handler_pc: continue_pc } => {
                tracing::debug!(wanted = %depth, current_depth = %callstack_depth, ?continue_pc, "revert");
                let Some(continue_pc) = *continue_pc else {
                    tracing::error!("revert missing stored continuations");
                    return
                };
                self.one_time_actions.push(FinishCycleOneTimeActions::ForceRevert {
                    error: reason.to_owned(),
                    exception_handler: continue_pc,
                });
                self.next_return_action = None;
            }
        }
    }

    fn start_prank<S: DatabaseExt + Send>(
        &mut self,
        storage: &StoragePtr<EraDb<S>>,
        sender: H160,
        origin: Option<H160>,
    ) {
        if self.permanent_actions.broadcast.is_some() {
            tracing::error!("prank is incompatible with broadcast");
            return
        }

        match origin {
            None => {
                self.permanent_actions.start_prank.replace(StartPrankOpts { sender, origin: None });
            }
            Some(tx_origin) => {
                let key = StorageKey::new(
                    AccountTreeId::new(zksync_types::SYSTEM_CONTEXT_ADDRESS),
                    zksync_types::SYSTEM_CONTEXT_TX_ORIGIN_POSITION,
                );
                let storage = &mut storage.borrow_mut();
                let original_tx_origin = storage.read_value(&key);
                self.write_storage(key, tx_origin.into(), storage);

                self.permanent_actions
                    .start_prank
                    .replace(StartPrankOpts { sender, origin: Some(original_tx_origin.into()) });
            }
        }
    }

    fn stop_prank<S: DatabaseExt + Send>(&mut self, storage: &StoragePtr<EraDb<S>>) {
        if let Some(original_tx_origin) =
            self.permanent_actions.start_prank.take().and_then(|v| v.origin)
        {
            let key = StorageKey::new(
                AccountTreeId::new(zksync_types::SYSTEM_CONTEXT_ADDRESS),
                zksync_types::SYSTEM_CONTEXT_TX_ORIGIN_POSITION,
            );
            self.write_storage(key, original_tx_origin.into(), &mut storage.borrow_mut());
        }
    }

    fn start_broadcast<S: DatabaseExt + Send>(
        &mut self,
        storage: &StoragePtr<EraDb<S>>,
        state: &VmLocalStateData<'_>,
        new_origin: Option<H160>,
    ) {
        if self.permanent_actions.start_prank.is_some() {
            tracing::error!("broadcast is incompatible with prank");
            return
        }

        let depth = state.vm_local_state.callstack.depth();

        let key = StorageKey::new(
            AccountTreeId::new(zksync_types::SYSTEM_CONTEXT_ADDRESS),
            zksync_types::SYSTEM_CONTEXT_TX_ORIGIN_POSITION,
        );

        let mut storage = storage.borrow_mut();

        let original_tx_origin = storage.read_value(&key);
        let new_origin = new_origin.unwrap_or(original_tx_origin.into());

        self.permanent_actions.broadcast = Some(BroadcastOpts {
            new_origin,
            original_origin: original_tx_origin.into(),
            original_caller: state.vm_local_state.callstack.current.msg_sender,
            depth,
        })
    }

    fn stop_broadcast(&mut self) {
        if let Some(broadcast) = self.permanent_actions.broadcast.take() {
            self.one_time_actions
                .push(FinishCycleOneTimeActions::SetOrigin { origin: broadcast.original_origin });
        }
    }

    /// Merge current modified storage with the entire storage modifications made so far in the test
    fn get_modified_storage(
        &self,
        storage: &HashMap<StorageKey, H256>,
    ) -> HashMap<StorageKey, H256> {
        let mut modified_storage = self
            .storage_modifications
            .keys
            .clone()
            .into_iter()
            .filter(|(key, _)| key.address() != &zksync_types::SYSTEM_CONTEXT_ADDRESS)
            .collect::<HashMap<_, _>>();
        modified_storage.extend(
            storage
                .iter()
                .filter(|(key, _)| key.address() != &zksync_types::SYSTEM_CONTEXT_ADDRESS),
        );
        modified_storage
    }

    /// Merge current modified bytecodes with the entire storage modifications made so far in the
    /// test
    fn get_modified_bytecodes(
        &self,
        bootloader_bytecodes: Vec<CompressedBytecodeInfo>,
    ) -> HashMap<H256, Vec<u8>> {
        let mut modified_bytecodes = self.storage_modifications.bytecodes.clone();
        modified_bytecodes.extend(
            bootloader_bytecodes
                .iter()
                .map(|b| {
                    let (bytecode_key, bytecode_value) =
                        bytecode_to_factory_dep(b.original.clone());
                    let key = u256_to_h256(bytecode_key);
                    let value = bytecode_value
                        .into_iter()
                        .flat_map(|v| u256_to_h256(v).as_bytes().to_owned())
                        .collect_vec();
                    (key, value)
                })
                .collect::<HashMap<_, _>>(),
        );
        modified_bytecodes
    }

    /// Initializes the EVM fork
    /// Translates all deployed contract bytecodes with EVM bytecodes
    /// Sets the EVM env to use for execution with the latest fork environment
    fn maybe_switch_vm<S: DatabaseExt + DatabaseCommit + Send>(
        &mut self,
        mut db: MutexGuard<'_, Box<S>>,
        fork_id: LocalForkId,
        force_initialized: bool,
    ) {
        let fork_info = db.get_fork_info(fork_id).expect("failed getting fork info");
        self.use_evm_fork = if fork_info.fork_type.is_evm() {
            tracing::info!("switch mode: EVM");

            if !self.initialized_evm_forks.contains(&fork_id) || force_initialized {
                // initialize all deployed contracts
                let mut changes = rHashMap::new();
                for (deployed_address, deployed_bytecode_hash) in
                    &self.storage_modifications.deployed_codes
                {
                    if let Some(contract) = self
                        .dual_compiled_contracts
                        .iter()
                        .find(|contract| &contract.zk_bytecode_hash == deployed_bytecode_hash)
                    {
                        tracing::info!("overwriting {deployed_address:?} with evm contract code");
                        let address = h160_to_address(*deployed_address);
                        let info = db
                            .basic(address)
                            .expect("failed getting account info")
                            .expect("expected some account info");

                        // code_hash is computed on commit
                        changes.insert(
                            address,
                            Account {
                                info: AccountInfo {
                                    balance: info.balance,
                                    nonce: info.nonce,
                                    code_hash: KECCAK_EMPTY,
                                    code: Some(Bytecode::new_raw(Bytes::from(
                                        contract.evm_deployed_bytecode.clone(),
                                    ))),
                                },
                                storage: Default::default(),
                                status: AccountStatus::Touched,
                            },
                        );
                    }
                }

                db.commit(changes);
                self.initialized_evm_forks.insert(fork_id);
            }

            Some(fork_info.fork_env.clone())
        } else {
            tracing::info!("switch mode: ZK");
            None
        };
    }

    /// Handles a FarCall within an EVM if the `use_evm_fork` is set.
    /// Returns true if the call was handled, false otherwise.
    fn maybe_handle_evm_far_call<H: HistoryMode, S: DatabaseExt + DatabaseCommit + Send>(
        &mut self,
        state: VmLocalStateData<'_>,
        memory: &SimpleMemory<H>,
        storage: StoragePtr<EraDb<S>>,
    ) -> bool {
        if let Some(fork_env) = &self.use_evm_fork {
            let current = state.vm_local_state.callstack.get_current_stack();

            let call = farcall::parse(&state, memory);

            if call.to() == &SYSTEM_CONTEXT_ADDRESS {
                if call.selector() == farcall::SELECTOR_SYSTEM_CONTEXT_BLOCK_NUMBER {
                    self.farcall_handler
                        .set_immediate_return(fork_env.block.number.to_be_bytes_vec());
                } else if call.selector() == farcall::SELECTOR_SYSTEM_CONTEXT_BLOCK_TIMESTAMP {
                    self.farcall_handler
                        .set_immediate_return(fork_env.block.timestamp.to_be_bytes_vec());
                }
                return true
            }

            if call.to() == &L2_ETH_TOKEN_ADDRESS &&
                call.selector() == farcall::SELECTOR_L2_ETH_BALANCE_OF
            {
                let account = call.params()[0];
                let account = h256_to_h160(&H256::from(account));

                let handle = &mut storage.borrow_mut();
                let era_db = &handle.storage_handle;
                let info =
                    era_db.db.lock().unwrap().basic(h160_to_address(account)).unwrap().unwrap();
                tracing::debug!("returning evm balance {:?} for {:?}", info.balance, account);

                self.farcall_handler.set_immediate_return(info.balance.to_be_bytes_vec());
                return true
            }

            if call.to() == &CONTRACT_DEPLOYER_ADDRESS &&
                (call.selector() == farcall::SELECTOR_CONTRACT_DEPLOYER_CREATE ||
                    call.selector() == farcall::SELECTOR_CONTRACT_DEPLOYER_CREATE2)
            {
                let salt = call.params()[0];
                // TODO translate bytecode hash to evm bytecode
                let bytecode_hash = H256::from(call.params()[1]);
                // Skip salt + bytecode_hash + offset + byte marker
                let input = call.param_bytes_after(4);

                let bytecode = self
                    .dual_compiled_contracts
                    .iter()
                    .find_map(|contract| {
                        if contract.zk_bytecode_hash == bytecode_hash {
                            Some(contract.evm_bytecode.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| {
                        panic!(
                            "cannot find matching bytecode to deploy for hash {:?}",
                            bytecode_hash
                        )
                    });

                let from = h160_to_address(current.msg_sender);
                let value = call.value();
                let data = [bytecode, input].concat();

                let era_env = self.env.get().unwrap();
                let mut env = into_revm_env(era_env);
                env.block = fork_env.block.clone();
                env.cfg.chain_id = fork_env.cfg.chain_id;
                env.cfg.disable_eip3607 = true;
                env.cfg.disable_base_fee = true;
                env.cfg.disable_block_gas_limit = true;
                let create_scheme = if call.selector() == farcall::SELECTOR_CONTRACT_DEPLOYER_CREATE
                {
                    CreateScheme::Create
                } else {
                    CreateScheme::Create2 { salt: rU256::from_be_bytes(salt) }
                };
                env.tx = revm::primitives::TxEnv {
                    caller: from,
                    transact_to: revm::primitives::TransactTo::Create(create_scheme),
                    data: data.into(),
                    value: u256_to_revm_u256(*value),
                    // As above, we set the gas price to 0.
                    gas_price: rU256::from(0),
                    gas_priority_fee: None,
                    gas_limit: era_env.system_env.gas_limit as u64,
                    ..env.tx
                };

                let handle = &mut storage.borrow_mut();
                let era_db = &handle.storage_handle;
                let mut db = era_db.db.lock().unwrap();

                let result = db.call_with_evm(env).expect("failed running evm call");

                let address = match result.result {
                    revm::primitives::ExecutionResult::Success {
                        output: revm::primitives::Output::Create(_, Some(address)),
                        ..
                    } => address,
                    _ => panic!("failed deploying contract: {:?}", result.result),
                };

                tracing::info!("deployed contract address: {:?}", address);
                let output = address.to_vec();

                // update state in current EVM after performing any storage translations
                if !result.state.is_empty() {
                    db.commit(result.state);
                }

                self.farcall_handler.set_immediate_return(output);
                return true
            }

            if !BROADCAST_IGNORED_CONTRACTS.contains(&current.code_address) {
                tracing::info!("Executing in EVM mode {:?}", call);

                let from = h160_to_address(current.msg_sender);
                let (value, to) = match call {
                    ParsedFarCall::SimpleCall { to, .. } => (rU256::ZERO, to),
                    ParsedFarCall::ValueCall { recipient, value, .. } => {
                        (u256_to_revm_u256(value), recipient)
                    }
                };
                // let to = h160_to_address(to);
                let calldata = call.calldata().to_vec();

                let era_env = self.env.get().unwrap();
                let mut env = into_revm_env(era_env);
                env.block = fork_env.block.clone();
                env.cfg.disable_eip3607 = true;
                env.cfg.disable_base_fee = true;
                env.cfg.disable_block_gas_limit = true;
                env.tx = revm::primitives::TxEnv {
                    caller: from,
                    transact_to: revm::primitives::TransactTo::Call(h160_to_address(to)),
                    data: calldata.into(),
                    value,
                    // As above, we set the gas price to 0.
                    gas_price: rU256::from(0),
                    gas_priority_fee: None,
                    gas_limit: era_env.system_env.gas_limit as u64,
                    ..env.tx
                };

                let era_db = &storage.borrow_mut().storage_handle;
                let mut db = era_db.db.lock().unwrap();

                let result = db.call_with_evm(env).expect("failed running evm call");
                if !result.result.is_success() {
                    panic!("failed deploying contract: {:?}", result.result);
                }

                let output = result.result.output().unwrap().clone();
                tracing::info!("result: {:?}", output);

                // update state in current EVM after performing any storage translations
                if !result.state.is_empty() {
                    db.commit(result.state);
                }

                self.farcall_handler.set_immediate_return(output.into());
                return true
            }
        }

        false
    }
}

trait ToZkEvmResult {
    /// Converts a [SolcValue] to return data for zkevm
    fn to_return_data(&self) -> Vec<U256>;
}

impl<T> ToZkEvmResult for T
where
    T: SolValue,
{
    fn to_return_data(&self) -> Vec<U256> {
        let abi_encoded_data = self.abi_encode();
        assert!(abi_encoded_data.len() % 32 == 0, "length must be multiple of 32");

        abi_encoded_data.chunks(32).map(U256::from_big_endian).collect_vec()
    }
}

fn into_revm_bytecodes(zk_bytecodes: HashMap<H256, Vec<u8>>) -> HashMap<U256, Vec<U256>> {
    zk_bytecodes
        .into_iter()
        .map(|(key, value)| {
            let key = h256_to_u256(key);
            let value = value.chunks(32).map(U256::from).collect_vec();
            (key, value)
        })
        .collect()
}

fn into_revm_env(env: &EraEnv) -> Env {
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
        let mut found = false;

        for actual_log in actual_iter.by_ref() {
            if are_logs_equal(expected_log, actual_log, &checks) {
                found = true;
                break
            }
        }

        if found {
            expected_iter.next(); // Move to the next expected log
        } else {
            // Expected log not found in the remaining actual logs
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

fn parse_env<F, T, E>(name: &str, parser_fn: F) -> eyre::Result<T>
where
    F: Fn(&str) -> Result<T, E>,
    E: std::error::Error + Send + Sync + 'static,
{
    let env_var = std::env::var(name).context("Env var not found")?;
    let env_var = parser_fn(&env_var)?;
    Ok(env_var)
}

fn parse_env_array<F, T, E>(name: &str, delim: &str, parser_fn: F) -> eyre::Result<Vec<T>>
where
    F: Fn(&str) -> Result<T, E>,
    E: std::error::Error + Send + Sync + 'static,
{
    let env_var = std::env::var(name).context("Env var not found")?;
    let env_vars = env_var
        .split(&delim)
        .map(|var| var.trim())
        .filter(|var| !var.is_empty())
        .map(parser_fn)
        .collect::<Result<Vec<_>, E>>()?;
    Ok(env_vars)
}
