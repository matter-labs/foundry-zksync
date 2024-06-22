//! Cheatcode EVM [Inspector].

use crate::{
    evm::{
        journaled_account,
        mapping::{self, MappingSlots},
        prank::Prank,
        DealRecord,
    },
    script::{Broadcast, ScriptWallets},
    test::expect::{self, ExpectedEmit, ExpectedRevert, ExpectedRevertKind},
    CheatsConfig, CheatsCtxt, Error, Result,
    Vm::{self, AccountAccess},
};

use alloy_primitives::{keccak256, Address, Bytes, Log, LogData, B256, U256, U64};
use alloy_rpc_types::request::{TransactionInput, TransactionRequest};
use alloy_sol_types::{SolInterface, SolValue};
use foundry_cheatcodes_common::{
    expect::{ExpectedCallData, ExpectedCallTracker, ExpectedCallType},
    mock::{MockCallDataContext, MockCallReturnData},
    record::RecordAccess,
};
use foundry_common::{evm::Breakpoints, provider::alloy::RpcUrl};
use foundry_evm_core::{
    backend::{DatabaseError, DatabaseExt, LocalForkId, RevertDiagnostic},
    constants::{
        CHEATCODE_ADDRESS, DEFAULT_CREATE2_DEPLOYER, DEFAULT_CREATE2_DEPLOYER_CODE,
        HARDHAT_CONSOLE_ADDRESS,
    },
};
use foundry_zksync_compiler::{DualCompiledContract, DualCompiledContracts};
use foundry_zksync_core::{
    convert::{ConvertAddress, ConvertH160, ConvertH256, ConvertRU256, ConvertU256},
    ZkTransactionMetadata,
};
use itertools::Itertools;
use revm::{
    interpreter::{
        opcode, CallInputs, CallScheme, CreateInputs, Gas, InstructionResult, Interpreter,
    },
    primitives::{
        AccountInfo, BlockEnv, Bytecode, CreateScheme, Env, ExecutionResult, HashMap as rHashMap,
        Output, StorageSlot, TransactTo, KECCAK_EMPTY,
    },
    EVMData, Inspector,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fs::File,
    io::BufReader,
    ops::Range,
    path::PathBuf,
    sync::Arc,
};
use zksync_types::{
    block::{pack_block_info, unpack_block_info},
    get_code_key, get_nonce_key,
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
    ACCOUNT_CODE_STORAGE_ADDRESS, CONTRACT_DEPLOYER_ADDRESS, CURRENT_VIRTUAL_BLOCK_INFO_POSITION,
    H256, KNOWN_CODES_STORAGE_ADDRESS, L2_BASE_TOKEN_ADDRESS, NONCE_HOLDER_ADDRESS,
    SYSTEM_CONTEXT_ADDRESS,
};

macro_rules! try_or_continue {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(_) => return,
        }
    };
}

/// Contains additional, test specific resources that should be kept for the duration of the test
#[derive(Debug, Default)]
pub struct Context {
    /// Buffered readers for files opened for reading (path => BufReader mapping)
    pub opened_read_files: HashMap<PathBuf, BufReader<File>>,
}

/// Every time we clone `Context`, we want it to be empty
impl Clone for Context {
    fn clone(&self) -> Self {
        Default::default()
    }
}

impl Context {
    /// Clears the context.
    #[inline]
    pub fn clear(&mut self) {
        self.opened_read_files.clear();
    }
}

/// Helps collecting transactions from different forks.
#[derive(Clone, Debug, Default)]
pub struct BroadcastableTransaction {
    /// The optional RPC URL.
    pub rpc: Option<RpcUrl>,
    /// The transaction to broadcast.
    pub transaction: TransactionRequest,
    /// ZK-VM factory deps
    pub zk_tx: Option<ZkTransactionMetadata>,
}

/// List of transactions that can be broadcasted.
pub type BroadcastableTransactions = VecDeque<BroadcastableTransaction>;

/// An EVM inspector that handles calls to various cheatcodes, each with their own behavior.
///
/// Cheatcodes can be called by contracts during execution to modify the VM environment, such as
/// mocking addresses, signatures and altering call reverts.
///
/// Executing cheatcodes can be very powerful. Most cheatcodes are limited to evm internals, but
/// there are also cheatcodes like `ffi` which can execute arbitrary commands or `writeFile` and
/// `readFile` which can manipulate files of the filesystem. Therefore, several restrictions are
/// implemented for these cheatcodes:
/// - `ffi`, and file cheatcodes are _always_ opt-in (via foundry config) and never enabled by
///   default: all respective cheatcode handlers implement the appropriate checks
/// - File cheatcodes require explicit permissions which paths are allowed for which operation, see
///   `Config.fs_permission`
/// - Only permitted accounts are allowed to execute cheatcodes in forking mode, this ensures no
///   contract deployed on the live network is able to execute cheatcodes by simply calling the
///   cheatcode address: by default, the caller, test contract and newly deployed contracts are
///   allowed to execute cheatcodes
#[derive(Clone, Debug, Default)]
pub struct Cheatcodes {
    /// The block environment
    ///
    /// Used in the cheatcode handler to overwrite the block environment separately from the
    /// execution block environment.
    pub block: Option<BlockEnv>,

    /// The gas price
    ///
    /// Used in the cheatcode handler to overwrite the gas price separately from the gas price
    /// in the execution environment.
    pub gas_price: Option<U256>,

    /// Address labels
    pub labels: HashMap<Address, String>,

    /// Remembered private keys
    pub script_wallets: Option<ScriptWallets>,

    /// Prank information
    pub prank: Option<Prank>,

    /// Expected revert information
    pub expected_revert: Option<ExpectedRevert>,

    /// Additional diagnostic for reverts
    pub fork_revert_diagnostic: Option<RevertDiagnostic>,

    /// Recorded storage reads and writes
    pub accesses: Option<RecordAccess>,

    /// Recorded account accesses (calls, creates) organized by relative call depth, where the
    /// topmost vector corresponds to accesses at the depth at which account access recording
    /// began. Each vector in the matrix represents a list of accesses at a specific call
    /// depth. Once that call context has ended, the last vector is removed from the matrix and
    /// merged into the previous vector.
    pub recorded_account_diffs_stack: Option<Vec<Vec<AccountAccess>>>,

    /// Recorded logs
    pub recorded_logs: Option<Vec<crate::Vm::Log>>,

    /// Mocked calls
    // **Note**: inner must a BTreeMap because of special `Ord` impl for `MockCallDataContext`
    pub mocked_calls: HashMap<Address, BTreeMap<MockCallDataContext, MockCallReturnData>>,

    /// Expected calls
    pub expected_calls: ExpectedCallTracker,
    /// Expected emits
    pub expected_emits: VecDeque<ExpectedEmit>,

    /// Map of context depths to memory offset ranges that may be written to within the call depth.
    pub allowed_mem_writes: HashMap<u64, Vec<Range<u64>>>,

    /// Current broadcasting information
    pub broadcast: Option<Broadcast>,

    /// Used to correct the nonce of --sender after the initiating call. For more, check
    /// `docs/scripting`.
    pub corrected_nonce: bool,

    /// Scripting based transactions
    pub broadcastable_transactions: BroadcastableTransactions,

    /// Additional, user configurable context this Inspector has access to when inspecting a call
    pub config: Arc<CheatsConfig>,

    /// Test-scoped context holding data that needs to be reset every test run
    pub context: Context,

    /// Whether to commit FS changes such as file creations, writes and deletes.
    /// Used to prevent duplicate changes file executing non-committing calls.
    pub fs_commit: bool,

    /// Serialized JSON values.
    // **Note**: both must a BTreeMap to ensure the order of the keys is deterministic.
    pub serialized_jsons: BTreeMap<String, BTreeMap<String, Value>>,

    /// All recorded ETH `deal`s.
    pub eth_deals: Vec<DealRecord>,

    /// Holds the stored gas info for when we pause gas metering. It is an `Option<Option<..>>`
    /// because the `call` callback in an `Inspector` doesn't get access to
    /// the `revm::Interpreter` which holds the `revm::Gas` struct that
    /// we need to copy. So we convert it to a `Some(None)` in `apply_cheatcode`, and once we have
    /// the interpreter, we copy the gas struct. Then each time there is an execution of an
    /// operation, we reset the gas.
    pub gas_metering: Option<Option<Gas>>,

    /// Holds stored gas info for when we pause gas metering, and we're entering/inside
    /// CREATE / CREATE2 frames. This is needed to make gas meter pausing work correctly when
    /// paused and creating new contracts.
    pub gas_metering_create: Option<Option<Gas>>,

    /// Mapping slots.
    pub mapping_slots: Option<HashMap<Address, MappingSlots>>,

    /// The current program counter.
    pub pc: usize,
    /// Breakpoints supplied by the `breakpoint` cheatcode.
    /// `char -> (address, pc)`
    pub breakpoints: Breakpoints,

    /// Use ZK-VM to execute CALLs and CREATEs.
    pub use_zk_vm: bool,

    /// Dual compiled contracts
    pub dual_compiled_contracts: DualCompiledContracts,

    /// Logs printed during ZK-VM execution.
    /// EVM logs have the value `None` so they can be interpolated later, since
    /// they are recorded by [foundry_evm::inspectors::LogCollector] tracer.
    pub combined_logs: Vec<Option<Log>>,

    /// Starts the cheatcode inspector in ZK mode.
    /// This is set to `false`, once the startup migration is completed.
    pub startup_zk: bool,

    /// The list of factory_deps seen so far during a test or script execution.
    /// Ideally these would be persisted in the storage, but since modifying [revm::JournaledState]
    /// would be a significant refactor, we maintain the factory_dep part in the [Cheatcodes].
    /// This can be done as each test runs with its own [Cheatcodes] instance, thereby
    /// providing the necessary level of isolation.
    pub persisted_factory_deps: HashMap<H256, Vec<u8>>,
}

impl Cheatcodes {
    /// Creates a new `Cheatcodes` with the given settings.
    #[inline]
    pub fn new(config: Arc<CheatsConfig>) -> Self {
        let labels = config.labels.clone();
        let script_wallets = config.script_wallets.clone();
        let mut dual_compiled_contracts = config.dual_compiled_contracts.clone();

        // We add the empty bytecode manually so it is correctly translated in zk mode.
        // This is used in many places in foundry, e.g. in cheatcode contract's account code.
        let empty_bytes = Bytes::from_static(&[0]);
        let zk_bytecode_hash = foundry_zksync_core::hash_bytecode(&foundry_zksync_core::EMPTY_CODE);
        let zk_deployed_bytecode = foundry_zksync_core::EMPTY_CODE.to_vec();

        dual_compiled_contracts.push(DualCompiledContract {
            name: String::from("EmptyEVMBytecode"),
            zk_bytecode_hash,
            zk_deployed_bytecode: zk_deployed_bytecode.clone(),
            zk_factory_deps: Default::default(),
            evm_bytecode_hash: B256::from_slice(&keccak256(&empty_bytes)[..]),
            evm_deployed_bytecode: Bytecode::new_raw(empty_bytes.clone())
                .to_checked()
                .bytecode
                .to_vec(),
            evm_bytecode: Bytecode::new_raw(empty_bytes).to_checked().bytecode.to_vec(),
        });

        let mut persisted_factory_deps = HashMap::new();
        persisted_factory_deps.insert(zk_bytecode_hash, zk_deployed_bytecode);

        let startup_zk = config.use_zk;
        Self {
            config,
            fs_commit: true,
            labels,
            script_wallets,
            dual_compiled_contracts,
            startup_zk,
            ..Default::default()
        }
    }

    fn apply_cheatcode<DB: DatabaseExt>(
        &mut self,
        data: &mut EVMData<'_, DB>,
        call: &CallInputs,
    ) -> Result {
        // decode the cheatcode call
        let decoded = Vm::VmCalls::abi_decode(&call.input, false)?;
        let caller = call.context.caller;

        // ensure the caller is allowed to execute cheatcodes,
        // but only if the backend is in forking mode
        data.db.ensure_cheatcode_access_forking_mode(&caller)?;

        apply_dispatch(&decoded, &mut CheatsCtxt { state: self, data, caller })
    }

    /// Determines the address of the contract and marks it as allowed
    /// Returns the address of the contract created
    ///
    /// There may be cheatcodes in the constructor of the new contract, in order to allow them
    /// automatically we need to determine the new address
    fn allow_cheatcodes_on_create<DB: DatabaseExt>(
        &self,
        data: &mut EVMData<'_, DB>,
        inputs: &CreateInputs,
    ) -> Address {
        let old_nonce = data
            .journaled_state
            .state
            .get(&inputs.caller)
            .map(|acc| acc.info.nonce)
            .unwrap_or_default();
        let created_address = inputs.created_address(old_nonce);
        if data.journaled_state.depth > 1 && !data.db.has_cheatcode_access(&inputs.caller) {
            // we only grant cheat code access for new contracts if the caller also has
            // cheatcode access and the new contract is created in top most call
            return created_address;
        }

        data.db.allow_cheatcode_access(created_address);

        created_address
    }

    /// Called when there was a revert.
    ///
    /// Cleanup any previously applied cheatcodes that altered the state in such a way that revm's
    /// revert would run into issues.
    pub fn on_revert<DB: DatabaseExt>(&mut self, data: &mut EVMData<'_, DB>) {
        trace!(deals=?self.eth_deals.len(), "rolling back deals");

        // Delay revert clean up until expected revert is handled, if set.
        if self.expected_revert.is_some() {
            return;
        }

        // we only want to apply cleanup top level
        if data.journaled_state.depth() > 0 {
            return;
        }

        // Roll back all previously applied deals
        // This will prevent overflow issues in revm's [`JournaledState::journal_revert`] routine
        // which rolls back any transfers.
        while let Some(record) = self.eth_deals.pop() {
            if let Some(acc) = data.journaled_state.state.get_mut(&record.address) {
                acc.info.balance = record.old_balance;
            }
        }
    }

    /// Selects the appropriate VM for the fork. Options: EVM, ZK-VM.
    /// CALL and CREATE are handled by the selected VM.
    ///
    /// Additionally:
    /// * Translates block information
    /// * Translates all persisted addresses
    pub fn select_fork_vm<DB: DatabaseExt>(
        &mut self,
        data: &mut EVMData<'_, DB>,
        fork_id: LocalForkId,
    ) {
        let fork_info = data.db.get_fork_info(fork_id).expect("failed getting fork info");
        if fork_info.fork_type.is_evm() {
            self.select_evm(data)
        } else {
            self.select_zk_vm(data, Some(&fork_info.fork_env))
        }
    }

    /// Switch to EVM and translate block info, balances, nonces and deployed codes for persistent
    /// accounts
    pub fn select_evm<DB: DatabaseExt>(&mut self, data: &mut EVMData<'_, DB>) {
        if !self.use_zk_vm {
            tracing::info!("already in EVM");
            return
        }

        tracing::info!("switching to EVM");
        self.use_zk_vm = false;

        let system_account = SYSTEM_CONTEXT_ADDRESS.to_address();
        journaled_account(data, system_account).expect("failed to load account");
        let balance_account = L2_BASE_TOKEN_ADDRESS.to_address();
        journaled_account(data, balance_account).expect("failed to load account");
        let nonce_account = NONCE_HOLDER_ADDRESS.to_address();
        journaled_account(data, nonce_account).expect("failed to load account");
        let account_code_account = ACCOUNT_CODE_STORAGE_ADDRESS.to_address();
        journaled_account(data, account_code_account).expect("failed to load account");

        // TODO we might need to store the deployment nonce under the contract storage
        // to not lose it across VMs.

        let block_info_key = CURRENT_VIRTUAL_BLOCK_INFO_POSITION.to_ru256();
        let (block_info, _) =
            data.journaled_state.sload(system_account, block_info_key, data.db).unwrap_or_default();
        let (block_number, block_timestamp) = unpack_block_info(block_info.to_u256());
        data.env.block.number = U256::from(block_number);
        data.env.block.timestamp = U256::from(block_timestamp);

        let test_contract = data.db.get_test_contract_address();
        for address in data.db.persistent_accounts() {
            info!(?address, "importing to evm state");

            let zk_address = address.to_h160();
            let balance_key = storage_key_for_eth_balance(&zk_address).key().to_ru256();
            let nonce_key = get_nonce_key(&zk_address).key().to_ru256();

            let (balance, _) = data
                .journaled_state
                .sload(balance_account, balance_key, data.db)
                .unwrap_or_default();
            let (full_nonce, _) =
                data.journaled_state.sload(nonce_account, nonce_key, data.db).unwrap_or_default();
            let (tx_nonce, _deployment_nonce) = decompose_full_nonce(full_nonce.to_u256());
            let nonce = tx_nonce.as_u64();

            let account_code_key = get_code_key(&zk_address).key().to_ru256();
            let (code_hash, code) = data
                .journaled_state
                .sload(account_code_account, account_code_key, data.db)
                .map(|(value, _)| value)
                .ok()
                .and_then(|zk_bytecode_hash| {
                    self.dual_compiled_contracts
                        .find_by_zk_bytecode_hash(zk_bytecode_hash.to_h256())
                        .map(|contract| {
                            (
                                contract.evm_bytecode_hash,
                                Some(Bytecode::new_raw(Bytes::from(
                                    contract.evm_deployed_bytecode.clone(),
                                ))),
                            )
                        })
                })
                .unwrap_or_else(|| (KECCAK_EMPTY, None));

            let account = journaled_account(data, address).expect("failed to load account");
            let _ = std::mem::replace(&mut account.info.balance, balance);
            let _ = std::mem::replace(&mut account.info.nonce, nonce);

            if test_contract.map(|addr| addr == address).unwrap_or_default() {
                tracing::trace!(?address, "ignoring code translation for test contract");
            } else {
                account.info.code_hash = code_hash;
                account.info.code = code.clone();
            }
        }
    }

    /// Switch to ZK-VM and translate block info, balances, nonces and deployed codes for persistent
    /// accounts
    pub fn select_zk_vm<DB: DatabaseExt>(
        &mut self,
        data: &mut EVMData<'_, DB>,
        new_env: Option<&Env>,
    ) {
        if self.use_zk_vm {
            tracing::info!("already in ZK-VM");
            return
        }

        tracing::info!("switching to ZK-VM");
        self.use_zk_vm = true;

        let env = new_env.unwrap_or(data.env);

        let mut system_storage: rHashMap<U256, StorageSlot> = Default::default();
        let block_info_key = CURRENT_VIRTUAL_BLOCK_INFO_POSITION.to_ru256();
        let block_info =
            pack_block_info(env.block.number.as_limbs()[0], env.block.timestamp.as_limbs()[0]);
        system_storage.insert(block_info_key, StorageSlot::new(block_info.to_ru256()));

        let mut l2_eth_storage: rHashMap<U256, StorageSlot> = Default::default();
        let mut nonce_storage: rHashMap<U256, StorageSlot> = Default::default();
        let mut account_code_storage: rHashMap<U256, StorageSlot> = Default::default();
        let mut known_codes_storage: rHashMap<U256, StorageSlot> = Default::default();
        let mut deployed_codes: HashMap<Address, AccountInfo> = Default::default();

        for address in data.db.persistent_accounts() {
            info!(?address, "importing to zk state");

            let account = journaled_account(data, address).expect("failed to load account");
            let info = &account.info;
            let zk_address = address.to_h160();

            let balance_key = storage_key_for_eth_balance(&zk_address).key().to_ru256();
            let nonce_key = get_nonce_key(&zk_address).key().to_ru256();
            l2_eth_storage.insert(balance_key, StorageSlot::new(info.balance));

            // TODO we need to find a proper way to handle deploy nonces instead of replicating
            let full_nonce = nonces_to_full_nonce(info.nonce.into(), info.nonce.into());
            nonce_storage.insert(nonce_key, StorageSlot::new(full_nonce.to_ru256()));

            if let Some(contract) = self.dual_compiled_contracts.iter().find(|contract| {
                info.code_hash != KECCAK_EMPTY && info.code_hash == contract.evm_bytecode_hash
            }) {
                account_code_storage.insert(
                    zk_address.to_h256().to_ru256(),
                    StorageSlot::new(contract.zk_bytecode_hash.to_ru256()),
                );
                known_codes_storage
                    .insert(contract.zk_bytecode_hash.to_ru256(), StorageSlot::new(U256::ZERO));

                let code_hash = B256::from_slice(contract.zk_bytecode_hash.as_bytes());
                deployed_codes.insert(
                    address,
                    AccountInfo {
                        balance: info.balance,
                        nonce: info.nonce,
                        code_hash,
                        code: Some(Bytecode::new_raw(Bytes::from(
                            contract.zk_deployed_bytecode.clone(),
                        ))),
                    },
                );
            } else {
                tracing::debug!("no zk contract found for {:?}", info.code_hash)
            }
        }

        let system_addr = SYSTEM_CONTEXT_ADDRESS.to_address();
        let system_account = journaled_account(data, system_addr).expect("failed to load account");
        system_account.storage.extend(system_storage.clone());

        let balance_addr = L2_BASE_TOKEN_ADDRESS.to_address();
        let balance_account =
            journaled_account(data, balance_addr).expect("failed to load account");
        balance_account.storage.extend(l2_eth_storage.clone());

        let nonce_addr = NONCE_HOLDER_ADDRESS.to_address();
        let nonce_account = journaled_account(data, nonce_addr).expect("failed to load account");
        nonce_account.storage.extend(nonce_storage.clone());

        let account_code_addr = ACCOUNT_CODE_STORAGE_ADDRESS.to_address();
        let account_code_account =
            journaled_account(data, account_code_addr).expect("failed to load account");
        account_code_account.storage.extend(account_code_storage.clone());

        let known_codes_addr = KNOWN_CODES_STORAGE_ADDRESS.to_address();
        let known_codes_account =
            journaled_account(data, known_codes_addr).expect("failed to load account");
        known_codes_account.storage.extend(known_codes_storage.clone());

        let test_contract = data.db.get_test_contract_address();
        for (address, info) in deployed_codes {
            let account = journaled_account(data, address).expect("failed to load account");
            let _ = std::mem::replace(&mut account.info.balance, info.balance);
            let _ = std::mem::replace(&mut account.info.nonce, info.nonce);
            if test_contract.map(|addr| addr == address).unwrap_or_default() {
                tracing::trace!(?address, "ignoring code translation for test contract");
            } else {
                account.info.code_hash = info.code_hash;
                account.info.code = info.code.clone();
            }
        }
    }
}

impl<DB: DatabaseExt + Send> Inspector<DB> for Cheatcodes {
    #[inline]
    fn initialize_interp(&mut self, _: &mut Interpreter<'_>, data: &mut EVMData<'_, DB>) {
        // When the first interpreter is initialized we've circumvented the balance and gas checks,
        // so we apply our actual block data with the correct fees and all.
        if let Some(block) = self.block.take() {
            data.env.block = block;
        }
        if let Some(gas_price) = self.gas_price.take() {
            data.env.tx.gas_price = gas_price;
        }
        if self.startup_zk && !self.use_zk_vm {
            self.startup_zk = false; // We only do this once.
            self.select_zk_vm(data, None);
        }
    }

    fn step_end(&mut self, interpreter: &mut Interpreter<'_>, data: &mut EVMData<'_, DB>) {
        // ovverride address(x).balance retrieval to make it consistent between EraVM and EVM
        if self.use_zk_vm {
            let address = match interpreter.current_opcode() {
                opcode::SELFBALANCE => interpreter.contract().address,
                opcode::BALANCE => {
                    if interpreter.stack.is_empty() {
                        interpreter.instruction_result = InstructionResult::StackUnderflow;
                        return;
                    }

                    Address::from_word(B256::from(unsafe { interpreter.stack.pop_unsafe() }))
                }
                _ => return,
            };

            // Safety: Length is checked above.
            let balance = foundry_zksync_core::balance(address, data.db, &mut data.journaled_state);

            // Skip the current BALANCE instruction since we've already handled it
            match interpreter.stack.push(balance) {
                Ok(_) => unsafe {
                    interpreter.instruction_pointer = interpreter.instruction_pointer.add(1);
                },
                Err(e) => {
                    interpreter.instruction_result = e;
                }
            }
        }
    }

    fn step(&mut self, interpreter: &mut Interpreter<'_>, data: &mut EVMData<'_, DB>) {
        self.pc = interpreter.program_counter();

        // reset gas if gas metering is turned off
        match self.gas_metering {
            Some(None) => {
                // need to store gas metering
                self.gas_metering = Some(Some(interpreter.gas));
            }
            Some(Some(gas)) => {
                match interpreter.current_opcode() {
                    opcode::CREATE | opcode::CREATE2 => {
                        // set we're about to enter CREATE frame to meter its gas on first opcode
                        // inside it
                        self.gas_metering_create = Some(None)
                    }
                    opcode::STOP | opcode::RETURN | opcode::SELFDESTRUCT | opcode::REVERT => {
                        // If we are ending current execution frame, we want to just fully reset gas
                        // otherwise weird things with returning gas from a call happen
                        // ref: https://github.com/bluealloy/revm/blob/2cb991091d32330cfe085320891737186947ce5a/crates/revm/src/evm_impl.rs#L190
                        //
                        // It would be nice if we had access to the interpreter in `call_end`, as we
                        // could just do this there instead.
                        match self.gas_metering_create {
                            None | Some(None) => {
                                interpreter.gas = Gas::new(0);
                            }
                            Some(Some(gas)) => {
                                // If this was CREATE frame, set correct gas limit. This is needed
                                // because CREATE opcodes deduct additional gas for code storage,
                                // and deducted amount is compared to gas limit. If we set this to
                                // 0, the CREATE would fail with out of gas.
                                //
                                // If we however set gas limit to the limit of outer frame, it would
                                // cause a panic after erasing gas cost post-create. Reason for this
                                // is pre-create REVM records `gas_limit - (gas_limit / 64)` as gas
                                // used, and erases costs by `remaining` gas post-create.
                                // gas used ref: https://github.com/bluealloy/revm/blob/2cb991091d32330cfe085320891737186947ce5a/crates/revm/src/instructions/host.rs#L254-L258
                                // post-create erase ref: https://github.com/bluealloy/revm/blob/2cb991091d32330cfe085320891737186947ce5a/crates/revm/src/instructions/host.rs#L279
                                interpreter.gas = Gas::new(gas.limit());

                                // reset CREATE gas metering because we're about to exit its frame
                                self.gas_metering_create = None
                            }
                        }
                    }
                    _ => {
                        // if just starting with CREATE opcodes, record its inner frame gas
                        if let Some(None) = self.gas_metering_create {
                            self.gas_metering_create = Some(Some(interpreter.gas))
                        }

                        // dont monitor gas changes, keep it constant
                        interpreter.gas = gas;
                    }
                }
            }
            _ => {}
        }

        // Record writes and reads if `record` has been called
        if let Some(storage_accesses) = &mut self.accesses {
            match interpreter.current_opcode() {
                opcode::SLOAD => {
                    let key = try_or_continue!(interpreter.stack().peek(0));
                    storage_accesses
                        .reads
                        .entry(interpreter.contract().address)
                        .or_default()
                        .push(key);
                }
                opcode::SSTORE => {
                    let key = try_or_continue!(interpreter.stack().peek(0));

                    // An SSTORE does an SLOAD internally
                    storage_accesses
                        .reads
                        .entry(interpreter.contract().address)
                        .or_default()
                        .push(key);
                    storage_accesses
                        .writes
                        .entry(interpreter.contract().address)
                        .or_default()
                        .push(key);
                }
                _ => (),
            }
        }

        // Record account access via SELFDESTRUCT if `recordAccountAccesses` has been called
        if let Some(account_accesses) = &mut self.recorded_account_diffs_stack {
            if interpreter.current_opcode() == opcode::SELFDESTRUCT {
                let target = try_or_continue!(interpreter.stack().peek(0));
                // load balance of this account
                let value = if let Ok((account, _)) =
                    data.journaled_state.load_account(interpreter.contract().address, data.db)
                {
                    account.info.balance
                } else {
                    U256::ZERO
                };
                let account = Address::from_word(B256::from(target));
                // get previous balance and initialized status of the target account
                let (initialized, old_balance) =
                    if let Ok((account, _)) = data.journaled_state.load_account(account, data.db) {
                        (account.info.exists(), account.info.balance)
                    } else {
                        (false, U256::ZERO)
                    };
                // register access for the target account
                let access = crate::Vm::AccountAccess {
                    chainInfo: crate::Vm::ChainInfo {
                        forkId: data.db.active_fork_id().unwrap_or_default(),
                        chainId: U256::from(data.env.cfg.chain_id),
                    },
                    accessor: interpreter.contract().address,
                    account,
                    kind: crate::Vm::AccountAccessKind::SelfDestruct,
                    initialized,
                    oldBalance: old_balance,
                    newBalance: old_balance + value,
                    value,
                    data: vec![],
                    reverted: false,
                    deployedCode: vec![],
                    storageAccesses: vec![],
                    depth: data.journaled_state.depth(),
                };
                // Ensure that we're not selfdestructing a context recording was initiated on
                if let Some(last) = account_accesses.last_mut() {
                    last.push(access);
                }
            }
        }

        // Record granular ordered storage accesses if `startStateDiffRecording` has been called
        if let Some(recorded_account_diffs_stack) = &mut self.recorded_account_diffs_stack {
            match interpreter.current_opcode() {
                opcode::SLOAD => {
                    let key = try_or_continue!(interpreter.stack().peek(0));
                    let address = interpreter.contract().address;

                    // Try to include present value for informational purposes, otherwise assume
                    // it's not set (zero value)
                    let mut present_value = U256::ZERO;
                    // Try to load the account and the slot's present value
                    if data.journaled_state.load_account(address, data.db).is_ok() {
                        if let Ok((previous, _)) = data.journaled_state.sload(address, key, data.db)
                        {
                            present_value = previous;
                        }
                    }
                    let access = crate::Vm::StorageAccess {
                        account: interpreter.contract().address,
                        slot: key.into(),
                        isWrite: false,
                        previousValue: present_value.into(),
                        newValue: present_value.into(),
                        reverted: false,
                    };
                    append_storage_access(
                        recorded_account_diffs_stack,
                        access,
                        data.journaled_state.depth(),
                    );
                }
                opcode::SSTORE => {
                    let key = try_or_continue!(interpreter.stack().peek(0));
                    let value = try_or_continue!(interpreter.stack().peek(1));
                    let address = interpreter.contract().address;
                    // Try to load the account and the slot's previous value, otherwise, assume it's
                    // not set (zero value)
                    let mut previous_value = U256::ZERO;
                    if data.journaled_state.load_account(address, data.db).is_ok() {
                        if let Ok((previous, _)) = data.journaled_state.sload(address, key, data.db)
                        {
                            previous_value = previous;
                        }
                    }

                    let access = crate::Vm::StorageAccess {
                        account: address,
                        slot: key.into(),
                        isWrite: true,
                        previousValue: previous_value.into(),
                        newValue: value.into(),
                        reverted: false,
                    };
                    append_storage_access(
                        recorded_account_diffs_stack,
                        access,
                        data.journaled_state.depth(),
                    );
                }
                // Record account accesses via the EXT family of opcodes
                opcode::EXTCODECOPY |
                opcode::EXTCODESIZE |
                opcode::EXTCODEHASH |
                opcode::BALANCE => {
                    let kind = match interpreter.current_opcode() {
                        opcode::EXTCODECOPY => crate::Vm::AccountAccessKind::Extcodecopy,
                        opcode::EXTCODESIZE => crate::Vm::AccountAccessKind::Extcodesize,
                        opcode::EXTCODEHASH => crate::Vm::AccountAccessKind::Extcodehash,
                        opcode::BALANCE => crate::Vm::AccountAccessKind::Balance,
                        _ => unreachable!(),
                    };
                    let address = Address::from_word(B256::from(try_or_continue!(interpreter
                        .stack()
                        .peek(0))));
                    let balance;
                    let initialized;
                    if let Ok((acc, _)) = data.journaled_state.load_account(address, data.db) {
                        initialized = acc.info.exists();
                        balance = acc.info.balance;
                    } else {
                        initialized = false;
                        balance = U256::ZERO;
                    }
                    let account_access = crate::Vm::AccountAccess {
                        chainInfo: crate::Vm::ChainInfo {
                            forkId: data.db.active_fork_id().unwrap_or_default(),
                            chainId: U256::from(data.env.cfg.chain_id),
                        },
                        accessor: interpreter.contract().address,
                        account: address,
                        kind,
                        initialized,
                        oldBalance: balance,
                        newBalance: balance,
                        value: U256::ZERO,
                        data: vec![],
                        reverted: false,
                        deployedCode: vec![],
                        storageAccesses: vec![],
                        depth: data.journaled_state.depth(),
                    };
                    // Record the EXT* call as an account access at the current depth
                    // (future storage accesses will be recorded in a new "Resume" context)
                    if let Some(last) = recorded_account_diffs_stack.last_mut() {
                        last.push(account_access);
                    } else {
                        recorded_account_diffs_stack.push(vec![account_access]);
                    }
                }
                _ => (),
            }
        }

        // If the allowed memory writes cheatcode is active at this context depth, check to see
        // if the current opcode can either mutate directly or expand memory. If the opcode at
        // the current program counter is a match, check if the modified memory lies within the
        // allowed ranges. If not, revert and fail the test.
        if let Some(ranges) = self.allowed_mem_writes.get(&data.journaled_state.depth()) {
            // The `mem_opcode_match` macro is used to match the current opcode against a list of
            // opcodes that can mutate memory (either directly or expansion via reading). If the
            // opcode is a match, the memory offsets that are being written to are checked to be
            // within the allowed ranges. If not, the test is failed and the transaction is
            // reverted. For all opcodes that can mutate memory aside from MSTORE,
            // MSTORE8, and MLOAD, the size and destination offset are on the stack, and
            // the macro expands all of these cases. For MSTORE, MSTORE8, and MLOAD, the
            // size of the memory write is implicit, so these cases are hard-coded.
            macro_rules! mem_opcode_match {
                ($(($opcode:ident, $offset_depth:expr, $size_depth:expr, $writes:expr)),* $(,)?) => {
                    match interpreter.current_opcode() {
                        ////////////////////////////////////////////////////////////////
                        //    OPERATIONS THAT CAN EXPAND/MUTATE MEMORY BY WRITING     //
                        ////////////////////////////////////////////////////////////////

                        opcode::MSTORE => {
                            // The offset of the mstore operation is at the top of the stack.
                            let offset = try_or_continue!(interpreter.stack().peek(0)).saturating_to::<u64>();

                            // If none of the allowed ranges contain [offset, offset + 32), memory has been
                            // unexpectedly mutated.
                            if !ranges.iter().any(|range| {
                                range.contains(&offset) && range.contains(&(offset + 31))
                            }) {
                                disallowed_mem_write(offset, 32, interpreter, ranges);
                                interpreter.instruction_result = InstructionResult::Revert;
                                return
                            }
                        }
                        opcode::MSTORE8 => {
                            // The offset of the mstore8 operation is at the top of the stack.
                            let offset = try_or_continue!(interpreter.stack().peek(0)).saturating_to::<u64>();

                            // If none of the allowed ranges contain the offset, memory has been
                            // unexpectedly mutated.
                            if !ranges.iter().any(|range| range.contains(&offset)) {
                                disallowed_mem_write(offset, 1, interpreter, ranges);
                                interpreter.instruction_result = InstructionResult::Revert;
                                return
                            }
                        }

                        ////////////////////////////////////////////////////////////////
                        //        OPERATIONS THAT CAN EXPAND MEMORY BY READING        //
                        ////////////////////////////////////////////////////////////////

                        opcode::MLOAD => {
                            // The offset of the mload operation is at the top of the stack
                            let offset = try_or_continue!(interpreter.stack().peek(0)).saturating_to::<u64>();

                            // If the offset being loaded is >= than the memory size, the
                            // memory is being expanded. If none of the allowed ranges contain
                            // [offset, offset + 32), memory has been unexpectedly mutated.
                            if offset >= interpreter.shared_memory.len() as u64 && !ranges.iter().any(|range| {
                                range.contains(&offset) && range.contains(&(offset + 31))
                            }) {
                                disallowed_mem_write(offset, 32, interpreter, ranges);
                                interpreter.instruction_result = InstructionResult::Revert;
                                return
                            }
                        }

                        ////////////////////////////////////////////////////////////////
                        //          OPERATIONS WITH OFFSET AND SIZE ON STACK          //
                        ////////////////////////////////////////////////////////////////

                        $(opcode::$opcode => {
                            // The destination offset of the operation is at the top of the stack.
                            let dest_offset = try_or_continue!(interpreter.stack().peek($offset_depth)).saturating_to::<u64>();

                            // The size of the data that will be copied is the third item on the stack.
                            let size = try_or_continue!(interpreter.stack().peek($size_depth)).saturating_to::<u64>();

                            // If none of the allowed ranges contain [dest_offset, dest_offset + size),
                            // memory outside of the expected ranges has been touched. If the opcode
                            // only reads from memory, this is okay as long as the memory is not expanded.
                            let fail_cond = !ranges.iter().any(|range| {
                                    range.contains(&dest_offset) &&
                                        range.contains(&(dest_offset + size.saturating_sub(1)))
                                }) && ($writes ||
                                    [dest_offset, (dest_offset + size).saturating_sub(1)].into_iter().any(|offset| {
                                        offset >= interpreter.shared_memory.len() as u64
                                    })
                                );

                            // If the failure condition is met, set the output buffer to a revert string
                            // that gives information about the allowed ranges and revert.
                            if fail_cond {
                                disallowed_mem_write(dest_offset, size, interpreter, ranges);
                                interpreter.instruction_result = InstructionResult::Revert;
                                return
                            }
                        })*
                        _ => ()
                    }
                }
            }

            // Check if the current opcode can write to memory, and if so, check if the memory
            // being written to is registered as safe to modify.
            mem_opcode_match!(
                (CALLDATACOPY, 0, 2, true),
                (CODECOPY, 0, 2, true),
                (RETURNDATACOPY, 0, 2, true),
                (EXTCODECOPY, 1, 3, true),
                (CALL, 5, 6, true),
                (CALLCODE, 5, 6, true),
                (STATICCALL, 4, 5, true),
                (DELEGATECALL, 4, 5, true),
                (KECCAK256, 0, 1, false),
                (LOG0, 0, 1, false),
                (LOG1, 0, 1, false),
                (LOG2, 0, 1, false),
                (LOG3, 0, 1, false),
                (LOG4, 0, 1, false),
                (CREATE, 1, 2, false),
                (CREATE2, 1, 2, false),
                (RETURN, 0, 1, false),
                (REVERT, 0, 1, false),
            )
        }

        // Record writes with sstore (and sha3) if `StartMappingRecording` has been called
        if let Some(mapping_slots) = &mut self.mapping_slots {
            mapping::step(mapping_slots, interpreter);
        }
    }

    fn log(&mut self, _: &mut EVMData<'_, DB>, address: &Address, topics: &[B256], data: &Bytes) {
        if !self.expected_emits.is_empty() {
            expect::handle_expect_emit(self, address, topics, data);
        }

        // Stores this log if `recordLogs` has been called
        if let Some(storage_recorded_logs) = &mut self.recorded_logs {
            storage_recorded_logs.push(Vm::Log {
                topics: topics.to_vec(),
                data: data.to_vec(),
                emitter: *address,
            });
        }

        self.combined_logs.push(None);
    }

    fn call(
        &mut self,
        data: &mut EVMData<'_, DB>,
        call: &mut CallInputs,
    ) -> (InstructionResult, Gas, Bytes) {
        let gas = Gas::new(call.gas_limit);

        if call.contract == CHEATCODE_ADDRESS {
            return match self.apply_cheatcode(data, call) {
                Ok(retdata) => (InstructionResult::Return, gas, retdata.into()),
                Err(err) => (InstructionResult::Revert, gas, err.abi_encode().into()),
            };
        }

        if call.contract == HARDHAT_CONSOLE_ADDRESS {
            self.combined_logs.push(None);

            return (InstructionResult::Continue, gas, Bytes::new());
        }

        // Handle expected calls

        // Grab the different calldatas expected.
        if let Some(expected_calls_for_target) = self.expected_calls.get_mut(&(call.contract)) {
            // Match every partial/full calldata
            for (calldata, (expected, actual_count)) in expected_calls_for_target {
                // Increment actual times seen if...
                // The calldata is at most, as big as this call's input, and
                if calldata.len() <= call.input.len() &&
                    // Both calldata match, taking the length of the assumed smaller one (which will have at least the selector), and
                    *calldata == call.input[..calldata.len()] &&
                    // The value matches, if provided
                    expected
                        .value
                        .map_or(true, |value| value == call.transfer.value) &&
                    // The gas matches, if provided
                    expected.gas.map_or(true, |gas| gas == call.gas_limit) &&
                    // The minimum gas matches, if provided
                    expected.min_gas.map_or(true, |min_gas| min_gas <= call.gas_limit)
                {
                    *actual_count += 1;
                }
            }
        }

        // Handle mocked calls
        if let Some(mocks) = self.mocked_calls.get(&call.contract) {
            let ctx = MockCallDataContext {
                calldata: call.input.clone(),
                value: Some(call.transfer.value),
            };
            if let Some(return_data) = mocks.get(&ctx).or_else(|| {
                mocks
                    .iter()
                    .find(|(mock, _)| {
                        call.input.get(..mock.calldata.len()) == Some(&mock.calldata[..]) &&
                            mock.value.map_or(true, |value| value == call.transfer.value)
                    })
                    .map(|(_, v)| v)
            }) {
                return (return_data.ret_type, gas, return_data.data.clone());
            }
        }

        // Apply our prank
        if let Some(prank) = &self.prank {
            if data.journaled_state.depth() >= prank.depth &&
                call.context.caller == prank.prank_caller
            {
                let mut prank_applied = false;

                // At the target depth we set `msg.sender`
                if data.journaled_state.depth() == prank.depth {
                    call.context.caller = prank.new_caller;
                    call.transfer.source = prank.new_caller;
                    prank_applied = true;
                }

                // At the target depth, or deeper, we set `tx.origin`
                if let Some(new_origin) = prank.new_origin {
                    data.env.tx.caller = new_origin;
                    prank_applied = true;
                }

                // If prank applied for first time, then update
                if prank_applied {
                    if let Some(applied_prank) = prank.first_time_applied() {
                        self.prank = Some(applied_prank);
                    }
                }
            }
        }

        // Apply our broadcast
        if let Some(broadcast) = &self.broadcast {
            // We only apply a broadcast *to a specific depth*.
            //
            // We do this because any subsequent contract calls *must* exist on chain and
            // we only want to grab *this* call, not internal ones
            if data.journaled_state.depth() == broadcast.depth &&
                call.context.caller == broadcast.original_caller
            {
                // At the target depth we set `msg.sender` & tx.origin.
                // We are simulating the caller as being an EOA, so *both* must be set to the
                // broadcast.origin.
                data.env.tx.caller = broadcast.new_origin;

                call.context.caller = broadcast.new_origin;
                call.transfer.source = broadcast.new_origin;
                // Add a `legacy` transaction to the VecDeque. We use a legacy transaction here
                // because we only need the from, to, value, and data. We can later change this
                // into 1559, in the cli package, relatively easily once we
                // know the target chain supports EIP-1559.
                if !call.is_static {
                    if let Err(err) =
                        data.journaled_state.load_account(broadcast.new_origin, data.db)
                    {
                        return (InstructionResult::Revert, gas, Error::encode(err));
                    }

                    let is_fixed_gas_limit = check_if_fixed_gas_limit(data, call.gas_limit);

                    let nonce = foundry_zksync_core::nonce(
                        broadcast.new_origin,
                        data.db,
                        &mut data.journaled_state,
                    ) as u64;

                    let account =
                        data.journaled_state.state().get_mut(&broadcast.new_origin).unwrap();

                    let zk_tx = if self.use_zk_vm {
                        // We shouldn't need factory_deps for CALLs
                        Some(ZkTransactionMetadata { factory_deps: Default::default() })
                    } else {
                        None
                    };

                    self.broadcastable_transactions.push_back(BroadcastableTransaction {
                        rpc: data.db.active_fork_url(),
                        transaction: TransactionRequest {
                            from: Some(broadcast.new_origin),
                            to: Some(call.contract),
                            value: Some(call.transfer.value),
                            input: TransactionInput::new(call.input.clone()),
                            nonce: Some(U64::from(nonce)),
                            gas: if is_fixed_gas_limit {
                                Some(U256::from(call.gas_limit))
                            } else {
                                None
                            },
                            ..Default::default()
                        },
                        zk_tx,
                    });
                    debug!(target: "cheatcodes", tx=?self.broadcastable_transactions.back().unwrap(), "broadcastable call");

                    let prev = account.info.nonce;

                    // Touch account to ensure that incremented nonce is committed
                    account.mark_touch();
                    account.info.nonce += 1;
                    debug!(target: "cheatcodes", address=%broadcast.new_origin, nonce=prev+1, prev, "incremented nonce");
                } else if broadcast.single_call {
                    let msg = "`staticcall`s are not allowed after `broadcast`; use `startBroadcast` instead";
                    return (InstructionResult::Revert, Gas::new(0), Error::encode(msg));
                }
            }
        }

        // Record called accounts if `startStateDiffRecording` has been called
        if let Some(recorded_account_diffs_stack) = &mut self.recorded_account_diffs_stack {
            // Determine if account is "initialized," ie, it has a non-zero balance, a non-zero
            // nonce, a non-zero KECCAK_EMPTY codehash, or non-empty code
            let initialized;
            let old_balance;
            if let Ok((acc, _)) = data.journaled_state.load_account(call.contract, data.db) {
                initialized = acc.info.exists();
                old_balance = acc.info.balance;
            } else {
                initialized = false;
                old_balance = U256::ZERO;
            }
            let kind = match call.context.scheme {
                CallScheme::Call => crate::Vm::AccountAccessKind::Call,
                CallScheme::CallCode => crate::Vm::AccountAccessKind::CallCode,
                CallScheme::DelegateCall => crate::Vm::AccountAccessKind::DelegateCall,
                CallScheme::StaticCall => crate::Vm::AccountAccessKind::StaticCall,
            };
            // Record this call by pushing it to a new pending vector; all subsequent calls at
            // that depth will be pushed to the same vector. When the call ends, the
            // RecordedAccountAccess (and all subsequent RecordedAccountAccesses) will be
            // updated with the revert status of this call, since the EVM does not mark accounts
            // as "warm" if the call from which they were accessed is reverted
            recorded_account_diffs_stack.push(vec![AccountAccess {
                chainInfo: crate::Vm::ChainInfo {
                    forkId: data.db.active_fork_id().unwrap_or_default(),
                    chainId: U256::from(data.env.cfg.chain_id),
                },
                accessor: call.context.caller,
                account: call.contract,
                kind,
                initialized,
                oldBalance: old_balance,
                newBalance: U256::ZERO, // updated on call_end
                value: call.transfer.value,
                data: call.input.to_vec(),
                reverted: false,
                deployedCode: vec![],
                storageAccesses: vec![], // updated on step
                depth: data.journaled_state.depth(),
            }]);
        }

        if self.use_zk_vm {
            if let TransactTo::Call(test_contract) = data.env.tx.transact_to {
                if call.contract == test_contract {
                    info!("using evm for calls to test contract {:?}", data.env);
                    return (InstructionResult::Continue, gas, Bytes::new())
                }
            }

            info!("running call in zk vm {:#?}", call);

            let ccx = foundry_zksync_core::vm::CheatcodeTracerContext {
                mocked_calls: self.mocked_calls.clone(),
                expected_calls: Some(&mut self.expected_calls),
                accesses: self.accesses.as_mut(),
                persisted_factory_deps: Some(&mut self.persisted_factory_deps),
            };
            if let Ok(result) = foundry_zksync_core::vm::call::<_, DatabaseError>(
                call,
                data.env,
                data.db,
                &mut data.journaled_state,
                ccx,
            ) {
                self.combined_logs.extend(result.logs.clone().into_iter().map(|log| {
                    Some(Log {
                        address: log.address,
                        data: LogData::new_unchecked(log.topics, log.data),
                    })
                }));
                //for each log in cloned logs call handle_expect_emit
                if !self.expected_emits.is_empty() {
                    for log in result.logs {
                        expect::handle_expect_emit(self, &log.address, &log.topics, &log.data);
                    }
                }

                return match result.execution_result {
                    ExecutionResult::Success { output, .. } => match output {
                        Output::Call(bytes) => (InstructionResult::Return, gas, bytes),
                        _ => (InstructionResult::Revert, gas, Bytes::new()),
                    },
                    ExecutionResult::Revert { output, .. } => {
                        (InstructionResult::Revert, gas, output)
                    }
                    ExecutionResult::Halt { .. } => (
                        InstructionResult::Revert,
                        gas,
                        Bytes::from_iter(String::from("zk vm halted").as_bytes()),
                    ),
                }
            }
        }

        (InstructionResult::Continue, gas, Bytes::new())
    }

    fn call_end(
        &mut self,
        data: &mut EVMData<'_, DB>,
        call: &CallInputs,
        remaining_gas: Gas,
        status: InstructionResult,
        retdata: Bytes,
    ) -> (InstructionResult, Gas, Bytes) {
        let cheatcode_call =
            call.contract == CHEATCODE_ADDRESS || call.contract == HARDHAT_CONSOLE_ADDRESS;

        // Clean up pranks/broadcasts if it's not a cheatcode call end. We shouldn't do
        // it for cheatcode calls because they are not appplied for cheatcodes in the `call` hook.
        // This should be placed before the revert handling, because we might exit early there
        if !cheatcode_call {
            // Clean up pranks
            if let Some(prank) = &self.prank {
                if data.journaled_state.depth() == prank.depth {
                    data.env.tx.caller = prank.prank_origin;

                    // Clean single-call prank once we have returned to the original depth
                    if prank.single_call {
                        let _ = self.prank.take();
                    }
                }
            }

            // Clean up broadcast
            if let Some(broadcast) = &self.broadcast {
                if data.journaled_state.depth() == broadcast.depth {
                    data.env.tx.caller = broadcast.original_origin;

                    // Clean single-call broadcast once we have returned to the original depth
                    if broadcast.single_call {
                        let _ = self.broadcast.take();
                    }
                }
            }
        }

        // Handle expected reverts
        if let Some(expected_revert) = &self.expected_revert {
            if data.journaled_state.depth() <= expected_revert.depth {
                let needs_processing: bool = match expected_revert.kind {
                    ExpectedRevertKind::Default => !cheatcode_call,
                    // `pending_processing` == true means that we're in the `call_end` hook for
                    // `vm.expectCheatcodeRevert` and shouldn't expect revert here
                    ExpectedRevertKind::Cheatcode { pending_processing } => {
                        cheatcode_call && !pending_processing
                    }
                };

                if needs_processing {
                    let expected_revert = std::mem::take(&mut self.expected_revert).unwrap();
                    return match expect::handle_expect_revert(
                        false,
                        expected_revert.reason.as_deref(),
                        status,
                        retdata,
                    ) {
                        Err(error) => {
                            trace!(expected=?expected_revert, ?error, ?status, "Expected revert mismatch");
                            (InstructionResult::Revert, remaining_gas, error.abi_encode().into())
                        }
                        Ok((_, retdata)) => (InstructionResult::Return, remaining_gas, retdata),
                    };
                }

                // Flip `pending_processing` flag for cheatcode revert expectations, marking that
                // we've exited the `expectCheatcodeRevert` call scope
                if let ExpectedRevertKind::Cheatcode { pending_processing } =
                    &mut self.expected_revert.as_mut().unwrap().kind
                {
                    if *pending_processing {
                        *pending_processing = false;
                    }
                }
            }
        }

        // Exit early for calls to cheatcodes as other logic is not relevant for cheatcode
        // invocations
        if cheatcode_call {
            return (status, remaining_gas, retdata);
        }

        // If `startStateDiffRecording` has been called, update the `reverted` status of the
        // previous call depth's recorded accesses, if any
        if let Some(recorded_account_diffs_stack) = &mut self.recorded_account_diffs_stack {
            // The root call cannot be recorded.
            if data.journaled_state.depth() > 0 {
                let mut last_recorded_depth =
                    recorded_account_diffs_stack.pop().expect("missing CALL account accesses");
                // Update the reverted status of all deeper calls if this call reverted, in
                // accordance with EVM behavior
                if status.is_revert() {
                    last_recorded_depth.iter_mut().for_each(|element| {
                        element.reverted = true;
                        element
                            .storageAccesses
                            .iter_mut()
                            .for_each(|storage_access| storage_access.reverted = true);
                    })
                }
                let call_access = last_recorded_depth.first_mut().expect("empty AccountAccesses");
                // Assert that we're at the correct depth before recording post-call state changes.
                // Depending on the depth the cheat was called at, there may not be any pending
                // calls to update if execution has percolated up to a higher depth.
                if call_access.depth == data.journaled_state.depth() {
                    if let Ok((acc, _)) = data.journaled_state.load_account(call.contract, data.db)
                    {
                        debug_assert!(access_is_call(call_access.kind));
                        call_access.newBalance = acc.info.balance;
                    }
                }
                // Merge the last depth's AccountAccesses into the AccountAccesses at the current
                // depth, or push them back onto the pending vector if higher depths were not
                // recorded. This preserves ordering of accesses.
                if let Some(last) = recorded_account_diffs_stack.last_mut() {
                    last.append(&mut last_recorded_depth);
                } else {
                    recorded_account_diffs_stack.push(last_recorded_depth);
                }
            }
        }

        // At the end of the call,
        // we need to check if we've found all the emits.
        // We know we've found all the expected emits in the right order
        // if the queue is fully matched.
        // If it's not fully matched, then either:
        // 1. Not enough events were emitted (we'll know this because the amount of times we
        // inspected events will be less than the size of the queue) 2. The wrong events
        // were emitted (The inspected events should match the size of the queue, but still some
        // events will not be matched)

        // First, check that we're at the call depth where the emits were declared from.
        let should_check_emits = self
            .expected_emits
            .iter()
            .any(|expected| expected.depth == data.journaled_state.depth()) &&
            // Ignore staticcalls
            !call.is_static;
        if should_check_emits {
            // Not all emits were matched.
            if self.expected_emits.iter().any(|expected| !expected.found) {
                return (
                    InstructionResult::Revert,
                    remaining_gas,
                    "log != expected log".abi_encode().into(),
                );
            } else {
                // All emits were found, we're good.
                // Clear the queue, as we expect the user to declare more events for the next call
                // if they wanna match further events.
                self.expected_emits.clear()
            }
        }

        // this will ensure we don't have false positives when trying to diagnose reverts in fork
        // mode
        let diag = self.fork_revert_diagnostic.take();

        // if there's a revert and a previous call was diagnosed as fork related revert then we can
        // return a better error here
        if status == InstructionResult::Revert {
            if let Some(err) = diag {
                return (status, remaining_gas, Error::encode(err.to_error_msg(&self.labels)));
            }
        }

        // try to diagnose reverts in multi-fork mode where a call is made to an address that does
        // not exist
        if let TransactTo::Call(test_contract) = data.env.tx.transact_to {
            // if a call to a different contract than the original test contract returned with
            // `Stop` we check if the contract actually exists on the active fork
            if data.db.is_forked_mode() &&
                status == InstructionResult::Stop &&
                call.contract != test_contract
            {
                self.fork_revert_diagnostic =
                    data.db.diagnose_revert(call.contract, &data.journaled_state);
            }
        }

        // If the depth is 0, then this is the root call terminating
        if data.journaled_state.depth() == 0 {
            // If we already have a revert, we shouldn't run the below logic as it can obfuscate an
            // earlier error that happened first with unrelated information about
            // another error when using cheatcodes.
            if status == InstructionResult::Revert {
                return (status, remaining_gas, retdata);
            }

            // If there's not a revert, we can continue on to run the last logic for expect*
            // cheatcodes. Match expected calls
            for (address, calldatas) in &self.expected_calls {
                // Loop over each address, and for each address, loop over each calldata it expects.
                for (calldata, (expected, actual_count)) in calldatas {
                    // Grab the values we expect to see
                    let ExpectedCallData { gas, min_gas, value, count, call_type } = expected;

                    let failed = match call_type {
                        // If the cheatcode was called with a `count` argument,
                        // we must check that the EVM performed a CALL with this calldata exactly
                        // `count` times.
                        ExpectedCallType::Count => *count != *actual_count,
                        // If the cheatcode was called without a `count` argument,
                        // we must check that the EVM performed a CALL with this calldata at least
                        // `count` times. The amount of times to check was
                        // the amount of time the cheatcode was called.
                        ExpectedCallType::NonCount => *count > *actual_count,
                    };
                    if failed {
                        let expected_values = [
                            Some(format!("data {}", hex::encode_prefixed(calldata))),
                            value.as_ref().map(|v| format!("value {v}")),
                            gas.map(|g| format!("gas {g}")),
                            min_gas.map(|g| format!("minimum gas {g}")),
                        ]
                        .into_iter()
                        .flatten()
                        .join(", ");
                        let but = if status.is_ok() {
                            let s = if *actual_count == 1 { "" } else { "s" };
                            format!("was called {actual_count} time{s}")
                        } else {
                            "the call reverted instead; \
                             ensure you're testing the happy path when using `expectCall`"
                                .to_string()
                        };
                        let s = if *count == 1 { "" } else { "s" };
                        let msg = format!(
                            "expected call to {address} with {expected_values} \
                             to be called {count} time{s}, but {but}"
                        );
                        return (InstructionResult::Revert, remaining_gas, Error::encode(msg));
                    }
                }
            }

            // Check if we have any leftover expected emits
            // First, if any emits were found at the root call, then we its ok and we remove them.
            self.expected_emits.retain(|expected| !expected.found);
            // If not empty, we got mismatched emits
            if !self.expected_emits.is_empty() {
                let msg = if status.is_ok() {
                    "expected an emit, but no logs were emitted afterwards. \
                     you might have mismatched events or not enough events were emitted"
                } else {
                    "expected an emit, but the call reverted instead. \
                     ensure you're testing the happy path when using `expectEmit`"
                };
                return (InstructionResult::Revert, remaining_gas, Error::encode(msg));
            }
        }

        (status, remaining_gas, retdata)
    }

    fn create(
        &mut self,
        data: &mut EVMData<'_, DB>,
        call: &mut CreateInputs,
    ) -> (InstructionResult, Option<Address>, Gas, Bytes) {
        let gas = Gas::new(call.gas_limit);

        // Apply our prank
        if let Some(prank) = &self.prank {
            if data.journaled_state.depth() >= prank.depth && call.caller == prank.prank_caller {
                // At the target depth we set `msg.sender`
                if data.journaled_state.depth() == prank.depth {
                    call.caller = prank.new_caller;
                }

                // At the target depth, or deeper, we set `tx.origin`
                if let Some(new_origin) = prank.new_origin {
                    data.env.tx.caller = new_origin;
                }
            }
        }

        // Apply our broadcast
        if let Some(broadcast) = &self.broadcast {
            if data.journaled_state.depth() >= broadcast.depth &&
                call.caller == broadcast.original_caller
            {
                if let Err(err) = data.journaled_state.load_account(broadcast.new_origin, data.db) {
                    return (InstructionResult::Revert, None, gas, Error::encode(err));
                }

                data.env.tx.caller = broadcast.new_origin;

                if data.journaled_state.depth() == broadcast.depth {
                    let (mut bytecode, mut to, mut nonce) = process_broadcast_create(
                        broadcast.new_origin,
                        call.init_code.clone(),
                        data,
                        call,
                    );
                    let is_fixed_gas_limit = check_if_fixed_gas_limit(data, call.gas_limit);

                    let zk_tx = if self.use_zk_vm {
                        to = Some(CONTRACT_DEPLOYER_ADDRESS.to_address());
                        nonce = foundry_zksync_core::nonce(
                            broadcast.new_origin,
                            data.db,
                            &mut data.journaled_state,
                        ) as u64;
                        let contract = self
                            .dual_compiled_contracts
                            .find_by_evm_bytecode(&call.init_code.0)
                            .unwrap_or_else(|| {
                                panic!("failed finding contract for {:?}", call.init_code)
                            });
                        let factory_deps =
                            self.dual_compiled_contracts.fetch_all_factory_deps(contract);

                        let constructor_input =
                            call.init_code[contract.evm_bytecode.len()..].to_vec();

                        let create_input = foundry_zksync_core::encode_create_params(
                            &call.scheme,
                            contract.zk_bytecode_hash,
                            constructor_input,
                        );
                        bytecode = Bytes::from(create_input);

                        Some(ZkTransactionMetadata { factory_deps })
                    } else {
                        None
                    };

                    self.broadcastable_transactions.push_back(BroadcastableTransaction {
                        rpc: data.db.active_fork_url(),
                        transaction: TransactionRequest {
                            from: Some(broadcast.new_origin),
                            to,
                            value: Some(call.value),
                            input: TransactionInput::new(bytecode),
                            nonce: Some(U64::from(nonce)),
                            gas: if is_fixed_gas_limit {
                                Some(U256::from(call.gas_limit))
                            } else {
                                None
                            },
                            ..Default::default()
                        },
                        zk_tx,
                    });
                    let kind = match call.scheme {
                        CreateScheme::Create => "create",
                        CreateScheme::Create2 { .. } => "create2",
                    };
                    debug!(target: "cheatcodes", tx=?self.broadcastable_transactions.back().unwrap(), "broadcastable {kind}");
                }
            }
        }

        // Apply the Create2 deployer
        if self.broadcast.is_some() || self.config.always_use_create_2_factory {
            match apply_create2_deployer(
                data,
                call,
                self.prank.as_ref(),
                self.broadcast.as_ref(),
                self.recorded_account_diffs_stack.as_mut(),
            ) {
                Ok(_) => {}
                Err(err) => return (InstructionResult::Revert, None, gas, Error::encode(err)),
            };
        }

        // allow cheatcodes from the address of the new contract
        // Compute the address *after* any possible broadcast updates, so it's based on the updated
        // call inputs
        let address = self.allow_cheatcodes_on_create(data, call);
        // If `recordAccountAccesses` has been called, record the create
        if let Some(recorded_account_diffs_stack) = &mut self.recorded_account_diffs_stack {
            // If the create scheme is create2, and the caller is the DEFAULT_CREATE2_DEPLOYER then
            // we must add 1 to the depth to account for the call to the create2 factory.
            let mut depth = data.journaled_state.depth();
            if let CreateScheme::Create2 { salt: _ } = call.scheme {
                if call.caller == DEFAULT_CREATE2_DEPLOYER {
                    depth += 1;
                }
            }

            // Record the create context as an account access and create a new vector to record all
            // subsequent account accesses
            recorded_account_diffs_stack.push(vec![AccountAccess {
                chainInfo: crate::Vm::ChainInfo {
                    forkId: data.db.active_fork_id().unwrap_or_default(),
                    chainId: U256::from(data.env.cfg.chain_id),
                },
                accessor: call.caller,
                account: address,
                kind: crate::Vm::AccountAccessKind::Create,
                initialized: true,
                oldBalance: U256::ZERO, // updated on create_end
                newBalance: U256::ZERO, // updated on create_end
                value: call.value,
                data: call.init_code.to_vec(),
                reverted: false,
                deployedCode: vec![],    // updated on create_end
                storageAccesses: vec![], // updated on create_end
                depth,
            }]);
        }

        if self.use_zk_vm {
            info!("running create in zk vm");
            if call.init_code.0 == DEFAULT_CREATE2_DEPLOYER_CODE {
                info!("ignoring DEFAULT_CREATE2_DEPLOYER_CODE for zk");
                return (InstructionResult::Continue, None, gas, Bytes::new())
            }

            let zk_contract = self
                .dual_compiled_contracts
                .find_by_evm_bytecode(&call.init_code.0)
                .unwrap_or_else(|| panic!("failed finding contract for {:?}", call.init_code));

            let factory_deps = self.dual_compiled_contracts.fetch_all_factory_deps(zk_contract);
            tracing::debug!(contract = zk_contract.name, "using dual compiled contract");

            let ccx = foundry_zksync_core::vm::CheatcodeTracerContext {
                mocked_calls: self.mocked_calls.clone(),
                expected_calls: Some(&mut self.expected_calls),
                accesses: self.accesses.as_mut(),
                persisted_factory_deps: Some(&mut self.persisted_factory_deps),
            };
            if let Ok(result) = foundry_zksync_core::vm::create::<_, DatabaseError>(
                call,
                zk_contract,
                factory_deps,
                data.env,
                data.db,
                &mut data.journaled_state,
                ccx,
            ) {
                self.combined_logs.extend(result.logs.clone().into_iter().map(|log| {
                    Some(Log {
                        address: log.address,
                        data: LogData::new_unchecked(log.topics, log.data),
                    })
                }));

                // for each log in cloned logs call handle_expect_emit
                if !self.expected_emits.is_empty() {
                    for log in result.logs {
                        expect::handle_expect_emit(self, &log.address, &log.topics, &log.data);
                    }
                }

                return match result.execution_result {
                    ExecutionResult::Success { output, .. } => match output {
                        Output::Create(bytes, address) => {
                            (InstructionResult::Return, address, gas, bytes)
                        }
                        _ => (InstructionResult::Revert, None, gas, Bytes::new()),
                    },
                    ExecutionResult::Revert { output, .. } => {
                        (InstructionResult::Revert, None, gas, output)
                    }
                    ExecutionResult::Halt { .. } => (
                        InstructionResult::Revert,
                        None,
                        gas,
                        Bytes::from_iter(String::from("zk vm halted").as_bytes()),
                    ),
                }
            }
        }

        (InstructionResult::Continue, None, gas, Bytes::new())
    }

    fn create_end(
        &mut self,
        data: &mut EVMData<'_, DB>,
        _: &CreateInputs,
        status: InstructionResult,
        address: Option<Address>,
        remaining_gas: Gas,
        retdata: Bytes,
    ) -> (InstructionResult, Option<Address>, Gas, Bytes) {
        // Clean up pranks
        if let Some(prank) = &self.prank {
            if data.journaled_state.depth() == prank.depth {
                data.env.tx.caller = prank.prank_origin;

                // Clean single-call prank once we have returned to the original depth
                if prank.single_call {
                    std::mem::take(&mut self.prank);
                }
            }
        }

        // Clean up broadcasts
        if let Some(broadcast) = &self.broadcast {
            if data.journaled_state.depth() == broadcast.depth {
                data.env.tx.caller = broadcast.original_origin;

                // Clean single-call broadcast once we have returned to the original depth
                if broadcast.single_call {
                    std::mem::take(&mut self.broadcast);
                }
            }
        }

        // Handle expected reverts
        if let Some(expected_revert) = &self.expected_revert {
            if data.journaled_state.depth() <= expected_revert.depth &&
                matches!(expected_revert.kind, ExpectedRevertKind::Default)
            {
                let expected_revert = std::mem::take(&mut self.expected_revert).unwrap();
                return match expect::handle_expect_revert(
                    true,
                    expected_revert.reason.as_deref(),
                    status,
                    retdata,
                ) {
                    Ok((address, retdata)) => {
                        (InstructionResult::Return, address, remaining_gas, retdata)
                    }
                    Err(err) => {
                        (InstructionResult::Revert, None, remaining_gas, err.abi_encode().into())
                    }
                };
            }
        }

        // If `startStateDiffRecording` has been called, update the `reverted` status of the
        // previous call depth's recorded accesses, if any
        if let Some(recorded_account_diffs_stack) = &mut self.recorded_account_diffs_stack {
            // The root call cannot be recorded.
            if data.journaled_state.depth() > 0 {
                let mut last_depth =
                    recorded_account_diffs_stack.pop().expect("missing CREATE account accesses");
                // Update the reverted status of all deeper calls if this call reverted, in
                // accordance with EVM behavior
                if status.is_revert() {
                    last_depth.iter_mut().for_each(|element| {
                        element.reverted = true;
                        element
                            .storageAccesses
                            .iter_mut()
                            .for_each(|storage_access| storage_access.reverted = true);
                    })
                }
                let create_access = last_depth.first_mut().expect("empty AccountAccesses");
                // Assert that we're at the correct depth before recording post-create state
                // changes. Depending on what depth the cheat was called at, there
                // may not be any pending calls to update if execution has
                // percolated up to a higher depth.
                if create_access.depth == data.journaled_state.depth() {
                    debug_assert_eq!(
                        create_access.kind as u8,
                        crate::Vm::AccountAccessKind::Create as u8
                    );
                    if let Some(address) = address {
                        if let Ok((created_acc, _)) =
                            data.journaled_state.load_account(address, data.db)
                        {
                            create_access.newBalance = created_acc.info.balance;
                            create_access.deployedCode = created_acc
                                .info
                                .code
                                .clone()
                                .unwrap_or_default()
                                .original_bytes()
                                .into();
                        }
                    }
                }
                // Merge the last depth's AccountAccesses into the AccountAccesses at the current
                // depth, or push them back onto the pending vector if higher depths were not
                // recorded. This preserves ordering of accesses.
                if let Some(last) = recorded_account_diffs_stack.last_mut() {
                    last.append(&mut last_depth);
                } else {
                    recorded_account_diffs_stack.push(last_depth);
                }
            }
        }

        (status, address, remaining_gas, retdata)
    }
}

/// Helper that expands memory, stores a revert string pertaining to a disallowed memory write,
/// and sets the return range to the revert string's location in memory.
fn disallowed_mem_write(
    dest_offset: u64,
    size: u64,
    interpreter: &mut Interpreter<'_>,
    ranges: &[Range<u64>],
) {
    let revert_string = format!(
        "memory write at offset 0x{:02X} of size 0x{:02X} not allowed; safe range: {}",
        dest_offset,
        size,
        ranges.iter().map(|r| format!("(0x{:02X}, 0x{:02X}]", r.start, r.end)).join(" ∪ ")
    )
    .abi_encode();
    mstore_revert_string(interpreter, &revert_string);
}

/// Expands memory, stores a revert string, and sets the return range to the revert
/// string's location in memory.
fn mstore_revert_string(interpreter: &mut Interpreter<'_>, bytes: &[u8]) {
    let starting_offset = interpreter.shared_memory.len();
    interpreter.shared_memory.resize(starting_offset + bytes.len());
    interpreter.shared_memory.set_data(starting_offset, 0, bytes.len(), bytes);
    interpreter.return_offset = starting_offset;
    interpreter.return_len = interpreter.shared_memory.len() - starting_offset
}

/// Applies the default CREATE2 deployer for contract creation.
///
/// This function is invoked during the contract creation process and updates the caller of the
/// contract creation transaction to be the `DEFAULT_CREATE2_DEPLOYER` if the `CreateScheme` is
/// `Create2` and the current execution depth matches the depth at which the `prank` or `broadcast`
/// was started, or the default depth of 1 if no prank or broadcast is currently active.
///
/// Returns a `DatabaseError::MissingCreate2Deployer` if the `DEFAULT_CREATE2_DEPLOYER` account is
/// not found or if it does not have any associated bytecode.
fn apply_create2_deployer<DB: DatabaseExt>(
    data: &mut EVMData<'_, DB>,
    call: &mut CreateInputs,
    prank: Option<&Prank>,
    broadcast: Option<&Broadcast>,
    diffs_stack: Option<&mut Vec<Vec<AccountAccess>>>,
) -> Result<(), DB::Error> {
    if let CreateScheme::Create2 { salt } = call.scheme {
        let mut base_depth = 1;
        if let Some(prank) = &prank {
            base_depth = prank.depth;
        } else if let Some(broadcast) = &broadcast {
            base_depth = broadcast.depth;
        }

        // If the create scheme is Create2 and the depth equals the broadcast/prank/default
        // depth, then use the default create2 factory as the deployer
        if data.journaled_state.depth() == base_depth {
            // Record the call to the create2 factory in the state diff
            if let Some(recorded_account_diffs_stack) = diffs_stack {
                let calldata = [&salt.to_be_bytes::<32>()[..], &call.init_code[..]].concat();
                recorded_account_diffs_stack.push(vec![AccountAccess {
                    chainInfo: crate::Vm::ChainInfo {
                        forkId: data.db.active_fork_id().unwrap_or_default(),
                        chainId: U256::from(data.env.cfg.chain_id),
                    },
                    accessor: call.caller,
                    account: DEFAULT_CREATE2_DEPLOYER,
                    kind: crate::Vm::AccountAccessKind::Call,
                    initialized: true,
                    oldBalance: U256::ZERO, // updated on create_end
                    newBalance: U256::ZERO, // updated on create_end
                    value: call.value,
                    data: calldata,
                    reverted: false,
                    deployedCode: vec![],    // updated on create_end
                    storageAccesses: vec![], // updated on create_end
                    depth: data.journaled_state.depth(),
                }])
            }

            // Sanity checks for our CREATE2 deployer
            let info =
                &data.journaled_state.load_account(DEFAULT_CREATE2_DEPLOYER, data.db)?.0.info;
            match &info.code {
                Some(code) if code.is_empty() => return Err(DatabaseError::MissingCreate2Deployer),
                None if data.db.code_by_hash(info.code_hash)?.is_empty() => {
                    return Err(DatabaseError::MissingCreate2Deployer)
                }
                _ => {}
            }

            call.caller = DEFAULT_CREATE2_DEPLOYER;
        }
    }
    Ok(())
}

/// Processes the creation of a new contract when broadcasting, preparing the necessary data for the
/// transaction to deploy the contract.
///
/// Returns the transaction calldata and the target address.
///
/// If the CreateScheme is Create, then this function returns the input bytecode without
/// modification and no address since it will be filled in later. If the CreateScheme is Create2,
/// then this function returns the calldata for the call to the create2 deployer which must be the
/// salt and init code concatenated.
fn process_broadcast_create<DB: DatabaseExt>(
    broadcast_sender: Address,
    bytecode: Bytes,
    data: &mut EVMData<'_, DB>,
    call: &mut CreateInputs,
) -> (Bytes, Option<Address>, u64) {
    call.caller = broadcast_sender;
    match call.scheme {
        CreateScheme::Create => {
            (bytecode, None, data.journaled_state.account(broadcast_sender).info.nonce)
        }
        CreateScheme::Create2 { salt } => {
            // We have to increment the nonce of the user address, since this create2 will be done
            // by the create2_deployer
            let account = data.journaled_state.state().get_mut(&broadcast_sender).unwrap();
            let prev = account.info.nonce;
            // Touch account to ensure that incremented nonce is committed
            account.mark_touch();
            account.info.nonce += 1;
            debug!(target: "cheatcodes", address=%broadcast_sender, nonce=prev+1, prev, "incremented nonce in create2");
            // Proxy deployer requires the data to be `salt ++ init_code`
            let calldata = [&salt.to_be_bytes::<32>()[..], &bytecode[..]].concat();
            (calldata.into(), Some(DEFAULT_CREATE2_DEPLOYER), prev)
        }
    }
}

// Determines if the gas limit on a given call was manually set in the script and should therefore
// not be overwritten by later estimations
fn check_if_fixed_gas_limit<DB: DatabaseExt>(data: &EVMData<'_, DB>, call_gas_limit: u64) -> bool {
    // If the gas limit was not set in the source code it is set to the estimated gas left at the
    // time of the call, which should be rather close to configured gas limit.
    // TODO: Find a way to reliably make this determination.
    // For example by generating it in the compilation or EVM simulation process
    U256::from(data.env.tx.gas_limit) > data.env.block.gas_limit &&
        U256::from(call_gas_limit) <= data.env.block.gas_limit
        // Transfers in forge scripts seem to be estimated at 2300 by revm leading to "Intrinsic
        // gas too low" failure when simulated on chain
        && call_gas_limit > 2300
}

/// Dispatches the cheatcode call to the appropriate function.
fn apply_dispatch<DB: DatabaseExt>(calls: &Vm::VmCalls, ccx: &mut CheatsCtxt<DB>) -> Result {
    macro_rules! match_ {
        ($($variant:ident),*) => {
            match calls {
                $(Vm::VmCalls::$variant(cheat) => crate::Cheatcode::apply_traced(cheat, ccx),)*
            }
        };
    }
    vm_calls!(match_)
}

/// Returns true if the kind of account access is a call.
fn access_is_call(kind: crate::Vm::AccountAccessKind) -> bool {
    matches!(
        kind,
        crate::Vm::AccountAccessKind::Call |
            crate::Vm::AccountAccessKind::StaticCall |
            crate::Vm::AccountAccessKind::CallCode |
            crate::Vm::AccountAccessKind::DelegateCall
    )
}

/// Appends an AccountAccess that resumes the recording of the current context.
fn append_storage_access(
    accesses: &mut [Vec<AccountAccess>],
    storage_access: crate::Vm::StorageAccess,
    storage_depth: u64,
) {
    if let Some(last) = accesses.last_mut() {
        // Assert that there's an existing record for the current context.
        if !last.is_empty() && last.first().unwrap().depth < storage_depth {
            // Three cases to consider:
            // 1. If there hasn't been a context switch since the start of this context, then add
            //    the storage access to the current context record.
            // 2. If there's an existing Resume record, then add the storage access to it.
            // 3. Otherwise, create a new Resume record based on the current context.
            if last.len() == 1 {
                last.first_mut().unwrap().storageAccesses.push(storage_access);
            } else {
                let last_record = last.last_mut().unwrap();
                if last_record.kind as u8 == crate::Vm::AccountAccessKind::Resume as u8 {
                    last_record.storageAccesses.push(storage_access);
                } else {
                    let entry = last.first().unwrap();
                    let resume_record = crate::Vm::AccountAccess {
                        chainInfo: crate::Vm::ChainInfo {
                            forkId: entry.chainInfo.forkId,
                            chainId: entry.chainInfo.chainId,
                        },
                        accessor: entry.accessor,
                        account: entry.account,
                        kind: crate::Vm::AccountAccessKind::Resume,
                        initialized: entry.initialized,
                        storageAccesses: vec![storage_access],
                        reverted: entry.reverted,
                        // The remaining fields are defaults
                        oldBalance: U256::ZERO,
                        newBalance: U256::ZERO,
                        value: U256::ZERO,
                        data: vec![],
                        deployedCode: vec![],
                        depth: entry.depth,
                    };
                    last.push(resume_record);
                }
            }
        }
    }
}
