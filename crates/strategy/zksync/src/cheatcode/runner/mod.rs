use std::sync::Arc;

use alloy_primitives::{Address, B256, Bytes, TxKind, U256, map::HashMap};
use alloy_rpc_types::{
    BlobTransactionSidecar,
    request::{TransactionInput, TransactionRequest},
    serde_helpers::WithOtherFields,
};
use foundry_cheatcodes::{
    Broadcast, BroadcastableTransaction, BroadcastableTransactions, Cheatcodes, CheatcodesExecutor,
    CheatsConfig, CheatsCtxt, CommonCreateInput, DynCheatcode, Result,
    Vm::{self, AccountAccess, AccountAccessKind, ChainInfo, StorageAccess},
    journaled_account,
    strategy::{
        CheatcodeInspectorStrategyContext, CheatcodeInspectorStrategyExt,
        CheatcodeInspectorStrategyRunner, EvmCheatcodeInspectorStrategyRunner,
    },
};
use foundry_common::TransactionMaybeSigned;
use foundry_evm::{
    Env,
    backend::{DatabaseError, LocalForkId},
    constants::{DEFAULT_CREATE2_DEPLOYER, DEFAULT_CREATE2_DEPLOYER_CODE},
};
use foundry_evm_core::{ContextExt, Ecx, backend::DatabaseExt};
use foundry_zksync_core::{
    ACCOUNT_CODE_STORAGE_ADDRESS, CONTRACT_DEPLOYER_ADDRESS, DEFAULT_CREATE2_DEPLOYER_ZKSYNC,
    KNOWN_CODES_STORAGE_ADDRESS, L2_BASE_TOKEN_ADDRESS, NONCE_HOLDER_ADDRESS, PaymasterParams,
    ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY, ZkTransactionMetadata,
    convert::{ConvertAddress, ConvertH160, ConvertH256, ConvertRU256, ConvertU256},
    get_account_code_key, get_balance_key, get_nonce_key,
    state::parse_full_nonce,
};
use itertools::Itertools;
use revm::{
    bytecode::opcode as op,
    context::{
        CreateScheme, JournalTr,
        result::{ExecutionResult, Output},
    },
    context_interface::transaction::SignedAuthorization,
    interpreter::{
        CallInput, CallInputs, CallOutcome, CreateOutcome, Gas, InstructionResult, Interpreter,
        InterpreterResult, interpreter_types::Jumps,
    },
    primitives::{HashSet, KECCAK_EMPTY},
    state::{AccountInfo, Bytecode, EvmStorageSlot},
};
use tracing::{debug, error, info, trace, warn};
use zksync_types::{
    CURRENT_VIRTUAL_BLOCK_INFO_POSITION, H256, SYSTEM_CONTEXT_ADDRESS,
    block::{pack_block_info, unpack_block_info},
    utils::{decompose_full_nonce, nonces_to_full_nonce},
};

use crate::cheatcode::context::ZksyncCheatcodeInspectorStrategyContext;

mod cheatcode_handlers;
mod utils;

/// ZKsync implementation for [CheatcodeInspectorStrategyRunner].
#[derive(Debug, Default, Clone)]
pub struct ZksyncCheatcodeInspectorStrategyRunner;

impl ZksyncCheatcodeInspectorStrategyRunner {
    fn append_recorded_accesses(
        &self,
        state: &mut Cheatcodes,
        ecx: Ecx<'_, '_, '_>,
        account_accesses: Vec<foundry_zksync_core::vm::AccountAccess>,
    ) {
        if let Some(recorded_account_diffs_stack) = state.recorded_account_diffs_stack.as_mut() {
            // A duplicate entry is inserted on call/create start by the revm, and updated on
            // call/create end.
            //
            // If we are inside a nested call (stack depth > 1), the placeholder
            // lives in the *parent* frame.  Its index will be exactly the current
            // length of that parent vector (`len()`), so we record that length.
            //
            // If we are at the root (depth == 1), the placeholder is already the
            // last element of the root vector.  We therefore record `len() - 1`.
            //
            // `zksync_fix_recorded_accesses()` uses this index later to drop the
            // single duplicate.
            //
            // TODO(zk): This is currently a hack, as account access recording is
            // done in 4 parts - create/create_end and call/call_end. And these must all be
            // moved to strategy.
            let stack_insert_index = if recorded_account_diffs_stack.len() > 1 {
                recorded_account_diffs_stack
                    .get(recorded_account_diffs_stack.len() - 2)
                    .map_or(0, Vec::len)
            } else {
                // `len() - 1`
                recorded_account_diffs_stack.first().map_or(0, |v| v.len().saturating_sub(1))
            };

            if let Some(last) = recorded_account_diffs_stack.last_mut() {
                let ctx = get_context(state.strategy.context.as_mut());
                ctx.remove_recorded_access_at = Some(stack_insert_index);

                for record in account_accesses {
                    let access = AccountAccess {
                        chainInfo: ChainInfo {
                            forkId: ecx
                                .journaled_state
                                .database
                                .active_fork_id()
                                .unwrap_or_default(),
                            chainId: U256::from(ecx.cfg.chain_id),
                        },
                        accessor: record.accessor,
                        account: record.account,
                        kind: match record.kind {
                            foundry_zksync_core::vm::AccountAccessKind::Call => {
                                AccountAccessKind::Call
                            }
                            foundry_zksync_core::vm::AccountAccessKind::Create => {
                                AccountAccessKind::Create
                            }
                        },
                        initialized: true,
                        oldBalance: record.old_balance,
                        newBalance: record.new_balance,
                        value: record.value,
                        data: record.data,
                        reverted: false,
                        deployedCode: if record.deployed_bytecode_hash.is_zero() {
                            Default::default()
                        } else {
                            Bytes::from(record.deployed_bytecode_hash.0)
                        },
                        storageAccesses: record
                            .storage_accesses
                            .into_iter()
                            .map(|record| StorageAccess {
                                account: record.account,
                                slot: record.slot.to_b256(),
                                isWrite: record.is_write,
                                previousValue: record.previous_value.to_b256(),
                                newValue: record.new_value.to_b256(),
                                reverted: false,
                            })
                            .collect(),
                        depth: record.depth,
                        oldNonce: 0,
                        newNonce: 0,
                    };
                    last.push(access);
                }
            }
        }
    }
}

impl CheatcodeInspectorStrategyRunner for ZksyncCheatcodeInspectorStrategyRunner {
    fn base_contract_deployed(&self, ctx: &mut dyn CheatcodeInspectorStrategyContext) {
        let ctx = get_context(ctx);

        debug!("allowing startup storage migration");
        ctx.zk_startup_migration.allow();
    }

    fn apply_full(
        &self,
        cheatcode: &dyn DynCheatcode,
        ccx: &mut CheatsCtxt<'_, '_, '_, '_>,
        executor: &mut dyn CheatcodesExecutor,
    ) -> Result {
        self.apply_cheatcode_impl(cheatcode, ccx, executor)
    }

    fn record_broadcastable_create_transactions(
        &self,
        ctx: &mut dyn CheatcodeInspectorStrategyContext,
        config: Arc<CheatsConfig>,
        input: &dyn CommonCreateInput,
        ecx_inner: Ecx<'_, '_, '_>,
        broadcast: &Broadcast,
        broadcastable_transactions: &mut BroadcastableTransactions,
    ) {
        let ctx_zk = get_context(ctx);
        if !ctx_zk.using_zk_vm {
            return EvmCheatcodeInspectorStrategyRunner.record_broadcastable_create_transactions(
                ctx,
                config,
                input,
                ecx_inner,
                broadcast,
                broadcastable_transactions,
            );
        }

        let ctx = ctx_zk;

        let init_code = input.init_code();
        let to = Some(TxKind::Call(CONTRACT_DEPLOYER_ADDRESS.to_address()));
        let mut nonce = foundry_zksync_core::tx_nonce(broadcast.new_origin, ecx_inner) as u64;
        let find_contract = ctx
            .dual_compiled_contracts
            .find_bytecode(&init_code.0)
            .unwrap_or_else(|| panic!("failed finding contract for {init_code:?}"));

        let constructor_args = find_contract.constructor_args();
        let contract = find_contract.contract();

        let factory_deps = ctx.dual_compiled_contracts.fetch_all_factory_deps(contract);

        let create_input = foundry_zksync_core::encode_create_params(
            &input.scheme().unwrap_or(CreateScheme::Create),
            contract.zk_bytecode_hash,
            constructor_args.to_vec(),
        );
        let call_init_code = Bytes::from(create_input);

        let mut zk_tx_factory_deps = factory_deps;

        let paymaster_params = ctx.paymaster_params.clone().map(|paymaster_data| PaymasterParams {
            paymaster: paymaster_data.address.to_h160(),
            paymaster_input: paymaster_data.input.to_vec(),
        });

        let rpc = ecx_inner.journaled_state.database.active_fork_url();

        let injected_factory_deps = ctx
            .zk_use_factory_deps
            .iter()
            .map(|contract| {
                utils::get_artifact_code(
                    &ctx.dual_compiled_contracts,
                    ctx.using_zk_vm,
                    &config,
                    contract,
                    false,
                )
                .inspect(|_| info!(contract, "pushing factory dep"))
                .unwrap_or_else(|_| {
                    panic!("failed to get bytecode for factory deps contract {contract}")
                })
                .to_vec()
            })
            .collect_vec();
        zk_tx_factory_deps.extend(injected_factory_deps);
        let mut batched = foundry_zksync_core::vm::batch_factory_dependencies(zk_tx_factory_deps);
        debug!(batches = batched.len(), "splitting factory deps for broadcast");
        // the last batch is the final one that does the deployment
        zk_tx_factory_deps = batched.pop().expect("must have at least 1 item");

        for factory_deps in batched {
            let mut tx = WithOtherFields::new(TransactionRequest {
                from: Some(broadcast.new_origin),
                to: Some(TxKind::Call(Address::ZERO)),
                value: None,
                nonce: Some(nonce),
                ..Default::default()
            });
            tx.other.insert(
                ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY.to_string(),
                serde_json::to_value(ZkTransactionMetadata::new(
                    factory_deps,
                    paymaster_params.clone(),
                ))
                .expect("failed encoding json"),
            );

            broadcastable_transactions.push_back(BroadcastableTransaction {
                rpc: rpc.clone(),
                transaction: TransactionMaybeSigned::Unsigned(tx),
            });

            //update nonce for each tx
            nonce += 1;
        }

        let mut tx = WithOtherFields::new(TransactionRequest {
            from: Some(broadcast.new_origin),
            to,
            value: Some(input.value()),
            input: TransactionInput::new(call_init_code),
            nonce: Some(nonce),
            ..Default::default()
        });
        tx.other.insert(
            ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY.to_string(),
            serde_json::to_value(ZkTransactionMetadata::new(zk_tx_factory_deps, paymaster_params))
                .expect("failed encoding json"),
        );
        broadcastable_transactions.push_back(BroadcastableTransaction {
            rpc,
            transaction: TransactionMaybeSigned::Unsigned(tx),
        });
    }

    fn record_broadcastable_call_transactions(
        &self,
        ctx: &mut dyn CheatcodeInspectorStrategyContext,
        config: Arc<CheatsConfig>,
        call: &CallInputs,
        ecx_inner: Ecx<'_, '_, '_>,
        is_fixed_gas_limit: bool,
        broadcast: &Broadcast,
        broadcastable_transactions: &mut BroadcastableTransactions,
        active_delegations: Vec<SignedAuthorization>,
        active_blob_sidecar: Option<BlobTransactionSidecar>,
    ) {
        let ctx_zk = get_context(ctx);

        if !ctx_zk.using_zk_vm {
            return EvmCheatcodeInspectorStrategyRunner.record_broadcastable_call_transactions(
                ctx,
                config,
                call,
                ecx_inner,
                is_fixed_gas_limit,
                broadcast,
                broadcastable_transactions,
                active_delegations,
                active_blob_sidecar,
            );
        }

        let ctx = ctx_zk;

        let tx_nonce = foundry_zksync_core::tx_nonce(broadcast.new_origin, ecx_inner);

        let factory_deps = &mut ctx.set_deployer_call_input_factory_deps;
        let injected_factory_deps = ctx
            .zk_use_factory_deps
            .iter()
            .flat_map(|contract| {
                let artifact_code = utils::get_artifact_code(
                    &ctx.dual_compiled_contracts,
                    ctx.using_zk_vm,
                    &config,
                    contract,
                    false,
                )
                .inspect(|_| info!(contract, "pushing factory dep"))
                .unwrap_or_else(|_| {
                    panic!("failed to get bytecode for factory deps contract {contract}")
                })
                .to_vec();
                let res = ctx.dual_compiled_contracts.find_bytecode(&artifact_code).unwrap();
                ctx.dual_compiled_contracts.fetch_all_factory_deps(res.contract())
            })
            .collect_vec();
        factory_deps.extend(injected_factory_deps.clone());

        let paymaster_params = ctx.paymaster_params.clone().map(|paymaster_data| PaymasterParams {
            paymaster: paymaster_data.address.to_h160(),
            paymaster_input: paymaster_data.input.to_vec(),
        });
        let factory_deps = if call.target_address == DEFAULT_CREATE2_DEPLOYER_ZKSYNC {
            // We shouldn't need factory_deps for CALLs
            factory_deps.clone()
        } else {
            // For this case we use only the injected factory deps
            injected_factory_deps
        };
        let zk_tx = ZkTransactionMetadata::new(factory_deps, paymaster_params);

        let mut tx_req = TransactionRequest {
            from: Some(broadcast.new_origin),
            to: Some(TxKind::from(Some(call.target_address))),
            value: call.transfer_value(),
            input: TransactionInput::new(call.input.bytes(ecx_inner)),
            nonce: Some(tx_nonce as u64),
            chain_id: Some(ecx_inner.cfg.chain_id),
            gas: if is_fixed_gas_limit { Some(call.gas_limit) } else { None },
            ..Default::default()
        };

        match (!active_delegations.is_empty(), active_blob_sidecar) {
            (true, Some(_)) => {
                // Note(zk): We can't return a call outcome from here
                return;
            }
            (true, None) => {
                tx_req.authorization_list = Some(active_delegations);
                tx_req.sidecar = None;
            }
            (false, Some(blob_sidecar)) => {
                tx_req.sidecar = Some(blob_sidecar);
                tx_req.authorization_list = None;
            }
            (false, None) => {
                tx_req.sidecar = None;
                tx_req.authorization_list = None;
            }
        }
        let mut tx = WithOtherFields::new(tx_req);

        tx.other.insert(
            ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY.to_string(),
            serde_json::to_value(zk_tx).expect("failed encoding json"),
        );

        broadcastable_transactions.push_back(BroadcastableTransaction {
            rpc: ecx_inner.journaled_state.database.active_fork_url(),
            transaction: TransactionMaybeSigned::Unsigned(tx),
        });
        debug!(target: "cheatcodes", tx=?broadcastable_transactions.back().unwrap(), "broadcastable call");
    }

    fn post_initialize_interp(
        &self,
        ctx: &mut dyn CheatcodeInspectorStrategyContext,
        _interpreter: &mut Interpreter,
        ecx: Ecx<'_, '_, '_>,
    ) {
        let ctx = get_context(ctx);

        if ctx.zk_startup_migration.is_allowed() && !ctx.using_zk_vm {
            self.select_zk_vm(ctx, ecx, None);
            ctx.zk_startup_migration.done();
            debug!("startup zkEVM storage migration completed");
        }
    }

    /// Returns true if handled.
    fn pre_step_end(
        &self,
        ctx: &mut dyn CheatcodeInspectorStrategyContext,
        interpreter: &mut Interpreter,
        ecx: Ecx<'_, '_, '_>,
    ) -> bool {
        // override address(x).balance retrieval to make it consistent between EraVM and EVM
        let ctx = get_context(ctx);

        if !ctx.using_zk_vm {
            return false;
        }

        let address = match interpreter.bytecode.opcode() {
            op::SELFBALANCE => interpreter.input.target_address,
            op::BALANCE => {
                if interpreter.stack.is_empty() {
                    return true;
                }

                Address::from_word(B256::from(unsafe { interpreter.stack.pop_unsafe() }))
            }
            _ => return true,
        };

        // Safety: Length is checked above.
        let balance = foundry_zksync_core::balance(address, ecx);

        // Skip the current BALANCE instruction since we've already handled it
        if interpreter.stack.push(balance) {
            interpreter.bytecode.relative_jump(1);
        } else {
            // stack overflow; nothing else to do here
        }

        false
    }
}

impl CheatcodeInspectorStrategyExt for ZksyncCheatcodeInspectorStrategyRunner {
    fn zksync_record_create_address(
        &self,
        ctx: &mut dyn CheatcodeInspectorStrategyContext,
        outcome: &CreateOutcome,
    ) {
        let ctx = get_context(ctx);

        if ctx.record_next_create_address {
            ctx.record_next_create_address = false;
            if let Some(address) = outcome.address {
                ctx.skip_zk_vm_addresses.insert(address);
            }
        }
    }

    fn zksync_sync_nonce(
        &self,
        ctx: &mut dyn CheatcodeInspectorStrategyContext,
        sender: Address,
        nonce: u64,
        ecx: Ecx<'_, '_, '_>,
    ) {
        let ctx = get_context(ctx);
        // NOTE(zk): We sync with the nonce changes to ensure that the nonce matches
        if !ctx.using_zk_vm {
            foundry_zksync_core::cheatcodes::set_nonce(sender, U256::from(nonce), ecx);
        }
    }

    fn zksync_set_deployer_call_input(
        &self,
        ecx: Ecx<'_, '_, '_>,
        ctx: &mut dyn CheatcodeInspectorStrategyContext,
        call: &mut CallInputs,
    ) {
        let ctx = get_context(ctx);

        if call.target_address == DEFAULT_CREATE2_DEPLOYER && ctx.using_zk_vm {
            call.target_address = DEFAULT_CREATE2_DEPLOYER_ZKSYNC;
            call.bytecode_address = DEFAULT_CREATE2_DEPLOYER_ZKSYNC;

            let input = call.input.bytes(ecx);
            let (salt, init_code) = input.split_at(32);
            let find_contract = ctx
                .dual_compiled_contracts
                .find_bytecode(init_code)
                .unwrap_or_else(|| panic!("failed finding contract for {init_code:?}"));

            let constructor_args = find_contract.constructor_args();
            let contract = find_contract.contract();

            // store these for broadcast reasons
            ctx.set_deployer_call_input_factory_deps =
                ctx.dual_compiled_contracts.fetch_all_factory_deps(contract);

            let create_input = foundry_zksync_core::encode_create_params(
                &CreateScheme::Create2 { salt: U256::from_be_slice(salt) },
                contract.zk_bytecode_hash,
                constructor_args.to_vec(),
            );

            call.input = CallInput::Bytes(create_input.into());
        }
    }

    /// Try handling the `CREATE` within zkEVM.
    /// If `Some` is returned then the result must be returned immediately, else the call must be
    /// handled in EVM.
    fn zksync_try_create(
        &self,
        state: &mut Cheatcodes,
        ecx: Ecx<'_, '_, '_>,
        input: &dyn CommonCreateInput,
        executor: &mut dyn CheatcodesExecutor,
    ) -> Option<CreateOutcome> {
        let ctx = get_context(state.strategy.context.as_mut());

        if !ctx.using_zk_vm {
            return None;
        }

        if ctx.skip_zk_vm {
            ctx.skip_zk_vm = false; // handled the skip, reset flag
            ctx.record_next_create_address = true;
            info!("running create in EVM, instead of zkEVM (skipped)");
            return None;
        }

        let (db, journal, _) = ecx.as_db_env_and_journal();
        if let Some(CreateScheme::Create) = input.scheme() {
            let caller = input.caller();
            let nonce =
                journal.load_account(db, caller).expect("to load caller account").info.nonce;
            let address = caller.create(nonce);
            if ecx
                .journaled_state
                .database
                .get_test_contract_address()
                .map(|addr| address == addr)
                .unwrap_or_default()
            {
                info!("running create in EVM, instead of zkEVM (Test Contract) {:#?}", address);
                return None;
            }
        }

        let init_code = input.init_code();
        if init_code.0 == DEFAULT_CREATE2_DEPLOYER_CODE {
            info!("running create in EVM, instead of zkEVM (DEFAULT_CREATE2_DEPLOYER_CODE)");
            return None;
        }

        info!("running create in zkEVM");

        let zk_create = if ctx.evm_interpreter {
            foundry_zksync_core::vm::ZkCreateInputs {
                value: input.value().to_u256(),
                msg_sender: input.caller(),
                create_input: init_code.to_vec(),
                factory_deps: Default::default(),
            }
        } else {
            let find_contract = ctx
                .dual_compiled_contracts
                .find_bytecode(&init_code.0)
                .unwrap_or_else(|| panic!("failed finding contract for {init_code:?}"));

            let constructor_args = find_contract.constructor_args();
            let info = find_contract.info();
            let contract = find_contract.contract();

            let zk_create_input = foundry_zksync_core::encode_create_params(
                &input.scheme().unwrap_or(CreateScheme::Create),
                contract.zk_bytecode_hash,
                constructor_args.to_vec(),
            );

            let mut factory_deps = ctx.dual_compiled_contracts.fetch_all_factory_deps(contract);
            let injected_factory_deps =
                ctx.zk_use_factory_deps
                    .iter()
                    .flat_map(|contract| {
                        let artifact_code = utils::get_artifact_code(
                    &ctx.dual_compiled_contracts,
                    ctx.using_zk_vm,
                    &state.config,
                    contract,
                    false,
                )
                .inspect(|_| info!(contract, "pushing factory dep"))
                .unwrap_or_else(|_| {
                    panic!("failed to get bytecode for injected factory deps contract {contract}")
                })
                .to_vec();
                        let res =
                            ctx.dual_compiled_contracts.find_bytecode(&artifact_code).unwrap();
                        ctx.dual_compiled_contracts.fetch_all_factory_deps(res.contract())
                    })
                    .collect_vec();
            factory_deps.extend(injected_factory_deps);

            // NOTE(zk): Clear injected factory deps so that they are not sent on further
            // transactions
            ctx.zk_use_factory_deps.clear();
            tracing::debug!(contract = info.name, "using dual compiled contract");

            foundry_zksync_core::vm::ZkCreateInputs {
                value: input.value().to_u256(),
                msg_sender: input.caller(),
                create_input: zk_create_input,
                factory_deps,
            }
        };

        let ccx = foundry_zksync_core::vm::CheatcodeTracerContext {
            mocked_calls: state.mocked_calls.clone(),
            expected_calls: Some(&mut state.expected_calls),
            accesses: Some(&mut state.accesses),
            persisted_factory_deps: Some(&mut ctx.persisted_factory_deps),
            paymaster_data: ctx.paymaster_params.take(),
            zk_env: ctx.zk_env.clone(),
            record_storage_accesses: state.recorded_account_diffs_stack.is_some(),
            evm_interpreter: ctx.evm_interpreter,
        };

        let mut gas = Gas::new(input.gas_limit());
        match foundry_zksync_core::vm::create::<_, DatabaseError>(zk_create, ecx, ccx) {
            Ok(result) => {
                if let Some(recorded_logs) = &mut state.recorded_logs {
                    recorded_logs.extend(result.logs.clone().into_iter().map(|log| Vm::Log {
                        topics: log.data.topics().to_vec(),
                        data: log.data.data.clone(),
                        emitter: log.address,
                    }));
                }

                // append console logs from zkEVM to the current executor's LogTracer
                result.logs.iter().filter_map(foundry_evm::decode::decode_console_log).for_each(
                    |decoded_log| {
                        executor.console_log(
                            &mut CheatsCtxt {
                                state,
                                ecx,
                                gas_limit: input.gas_limit(),
                                caller: input.caller(),
                            },
                            &decoded_log,
                        );
                    },
                );

                // append traces
                executor.trace_zksync(state, ecx, Box::new(result.call_traces));

                // for each log in cloned logs call handle_expect_emit
                if !state.expected_emits.is_empty() {
                    for log in result.logs {
                        foundry_cheatcodes::handle_expect_emit(
                            state,
                            &log,
                            Some(&mut Default::default()),
                        );
                    }
                }

                // We only increment the depth by one because that is sufficient to signal the check
                // in handle_expect_revert that the call has happened at a depth
                // deeper than the cheatcode, therefore tracking the depth in zkEVM
                // calls is not necessary. Normally adjusting the max depth would happen in
                // initialize_interp for each EVM call.
                if let Some(expected_revert) = &mut state.expected_revert {
                    expected_revert.max_depth =
                        std::cmp::max(ecx.journaled_state.depth() + 1, expected_revert.max_depth);
                }

                if result.execution_result.is_success() {
                    // record immutable variables
                    for (addr, imm_values) in result.recorded_immutables {
                        let addr = addr.to_address();
                        let keys = imm_values
                            .into_keys()
                            .map(|slot_index| {
                                foundry_zksync_core::get_immutable_slot_key(addr, slot_index)
                                    .to_ru256()
                            })
                            .collect::<HashSet<_>>();
                        let strategy = ecx.journaled_state.database.get_strategy();
                        strategy.runner.zksync_save_immutable_storage(
                            strategy.context.as_mut(),
                            addr,
                            keys,
                        );
                    }

                    // record storage accesses
                    self.append_recorded_accesses(state, ecx, result.account_accesses);
                }

                match result.execution_result {
                    ExecutionResult::Success { output, gas_used, .. } => {
                        let _ = gas.record_cost(gas_used);
                        match output {
                            Output::Create(bytes, address) => Some(CreateOutcome {
                                result: InterpreterResult {
                                    result: InstructionResult::Return,
                                    output: bytes,
                                    gas,
                                },
                                address,
                            }),
                            _ => Some(CreateOutcome {
                                result: InterpreterResult {
                                    result: InstructionResult::Revert,
                                    output: Bytes::new(),
                                    gas,
                                },
                                address: None,
                            }),
                        }
                    }
                    ExecutionResult::Revert { output, gas_used, .. } => {
                        let _ = gas.record_cost(gas_used);
                        Some(CreateOutcome {
                            result: InterpreterResult {
                                result: InstructionResult::Revert,
                                output,
                                gas,
                            },
                            address: None,
                        })
                    }
                    ExecutionResult::Halt { .. } => Some(CreateOutcome {
                        result: InterpreterResult {
                            result: InstructionResult::Revert,
                            output: Bytes::from_iter(String::from("zk vm halted").as_bytes()),
                            gas,
                        },
                        address: None,
                    }),
                }
            }
            Err(err) => {
                error!("error inspecting zkEVM: {err:?}");
                Some(CreateOutcome {
                    result: InterpreterResult {
                        result: InstructionResult::Revert,
                        output: Bytes::from_iter(
                            format!("error inspecting zkEVM: {err:?}").as_bytes(),
                        ),
                        gas,
                    },
                    address: None,
                })
            }
        }
    }

    /// Try handling the `CALL` within zkEVM.
    /// If `Some` is returned then the result must be returned immediately, else the call must be
    /// handled in EVM.
    fn zksync_try_call(
        &self,
        state: &mut Cheatcodes,
        ecx: Ecx<'_, '_, '_>,
        call: &CallInputs,
        executor: &mut dyn CheatcodesExecutor,
    ) -> Option<CallOutcome> {
        let ctx = get_context(state.strategy.context.as_mut());

        // We need to clear them out for the next call.
        let factory_deps = std::mem::take(&mut ctx.set_deployer_call_input_factory_deps);

        if !ctx.using_zk_vm {
            return None;
        }

        // also skip if the target was created during a zkEVM skip
        ctx.skip_zk_vm = ctx.skip_zk_vm || ctx.skip_zk_vm_addresses.contains(&call.target_address);
        if ctx.skip_zk_vm {
            ctx.skip_zk_vm = false; // handled the skip, reset flag
            info!("running create in EVM, instead of zkEVM (skipped) {:#?}", call);
            return None;
        }

        if ecx
            .journaled_state
            .database
            .get_test_contract_address()
            .map(|addr| call.bytecode_address == addr)
            .unwrap_or_default()
        {
            info!(
                "running call in EVM, instead of zkEVM (Test Contract) {:#?}",
                call.bytecode_address
            );
            return None;
        }

        info!("running call in zkEVM {:#?}", call);

        // NOTE(zk): Clear injected factory deps here even though it's actually used in broadcast.
        // To be consistent with where we clear factory deps in try_create_in_zk.
        ctx.zk_use_factory_deps.clear();

        let ccx = foundry_zksync_core::vm::CheatcodeTracerContext {
            mocked_calls: state.mocked_calls.clone(),
            expected_calls: Some(&mut state.expected_calls),
            accesses: Some(&mut state.accesses),
            persisted_factory_deps: Some(&mut ctx.persisted_factory_deps),
            paymaster_data: ctx.paymaster_params.take(),
            zk_env: ctx.zk_env.clone(),
            record_storage_accesses: state.recorded_account_diffs_stack.is_some(),
            evm_interpreter: ctx.evm_interpreter,
        };

        let mut gas = Gas::new(call.gas_limit);
        match foundry_zksync_core::vm::call::<_, DatabaseError>(call, factory_deps, ecx, ccx) {
            Ok(result) => {
                // append console logs from zkEVM to the current executor's LogTracer
                result.logs.iter().filter_map(foundry_evm::decode::decode_console_log).for_each(
                    |decoded_log| {
                        executor.console_log(
                            &mut CheatsCtxt {
                                state,
                                ecx,
                                gas_limit: call.gas_limit,
                                caller: call.caller,
                            },
                            &decoded_log,
                        );
                    },
                );

                // skip log processing for static calls
                if !call.is_static {
                    if let Some(recorded_logs) = &mut state.recorded_logs {
                        recorded_logs.extend(result.logs.clone().into_iter().map(|log| Vm::Log {
                            topics: log.data.topics().to_vec(),
                            data: log.data.data.clone(),
                            emitter: log.address,
                        }));
                    }

                    // append traces
                    executor.trace_zksync(state, ecx, Box::new(result.call_traces));

                    // for each log in cloned logs call handle_expect_emit
                    if !state.expected_emits.is_empty() {
                        for log in result.logs {
                            foundry_cheatcodes::handle_expect_emit(
                                state,
                                &log,
                                Some(&mut Default::default()),
                            );
                        }
                    }
                }

                // We only increment the depth by one because that is sufficient to signal the check
                // in handle_expect_revert that the call has happened at a depth
                // deeper than the cheatcode, therefore tracking the depth in zkEVM
                // calls is not necessary. Normally adjusting the max depth would happen in
                // initialize_interp for each EVM call.
                if let Some(expected_revert) = &mut state.expected_revert {
                    expected_revert.max_depth =
                        std::cmp::max(ecx.journaled_state.depth() + 1, expected_revert.max_depth);
                }

                if result.execution_result.is_success() {
                    // record storage accesses
                    self.append_recorded_accesses(state, ecx, result.account_accesses);
                }

                match result.execution_result {
                    ExecutionResult::Success { output, gas_used, .. } => {
                        let _ = gas.record_cost(gas_used);
                        match output {
                            Output::Call(bytes) => Some(CallOutcome {
                                result: InterpreterResult {
                                    result: InstructionResult::Return,
                                    output: bytes,
                                    gas,
                                },
                                memory_offset: call.return_memory_offset.clone(),
                                was_precompile_called: false,
                                precompile_call_logs: vec![],
                            }),
                            _ => Some(CallOutcome {
                                result: InterpreterResult {
                                    result: InstructionResult::Revert,
                                    output: Bytes::new(),
                                    gas,
                                },
                                memory_offset: call.return_memory_offset.clone(),
                                was_precompile_called: false,
                                precompile_call_logs: vec![],
                            }),
                        }
                    }
                    ExecutionResult::Revert { output, gas_used, .. } => {
                        let _ = gas.record_cost(gas_used);
                        Some(CallOutcome {
                            result: InterpreterResult {
                                result: InstructionResult::Revert,
                                output,
                                gas,
                            },
                            memory_offset: call.return_memory_offset.clone(),
                            was_precompile_called: false,
                            precompile_call_logs: vec![],
                        })
                    }
                    ExecutionResult::Halt { .. } => Some(CallOutcome {
                        result: InterpreterResult {
                            result: InstructionResult::Revert,
                            output: Bytes::from_iter(String::from("zk vm halted").as_bytes()),
                            gas,
                        },
                        memory_offset: call.return_memory_offset.clone(),
                        was_precompile_called: false,
                        precompile_call_logs: vec![],
                    }),
                }
            }
            Err(err) => {
                error!("error inspecting zkEVM: {err:?}");
                Some(CallOutcome {
                    result: InterpreterResult {
                        result: InstructionResult::Revert,
                        output: Bytes::from_iter(
                            format!("error inspecting zkEVM: {err:?}").as_bytes(),
                        ),
                        gas,
                    },
                    memory_offset: call.return_memory_offset.clone(),
                    was_precompile_called: false,
                    precompile_call_logs: vec![],
                })
            }
        }
    }

    fn zksync_remove_duplicate_account_access(&self, state: &mut Cheatcodes) {
        let ctx = get_context(state.strategy.context.as_mut());

        if let Some(index) = ctx.remove_recorded_access_at.take()
            && let Some(recorded_account_diffs_stack) = state.recorded_account_diffs_stack.as_mut()
            && let Some(last) = recorded_account_diffs_stack.last_mut()
        {
            // This entry has been inserted during CREATE/CALL operations in revm's
            // cheatcode inspector and must be removed.
            if index < last.len() {
                let _ = last.remove(index);
            } else {
                warn!(target: "zksync", index, len = last.len(), "skipping duplicate access removal: out of bounds");
            }
        }
    }

    /// Increments the EraVM transaction nonce after recording broadcastable txs
    /// and if we are not in isolate mode, as that handles it already
    fn zksync_increment_nonce_after_broadcast(
        &self,
        state: &mut Cheatcodes,
        ecx: Ecx<'_, '_, '_>,
        is_static: bool,
    ) {
        // Don't do anything for static calls
        if is_static {
            return;
        }

        // Explicitly increment tx nonce if calls are not isolated and we are broadcasting
        // This isn't needed in EVM, but required in zkEVM as the nonces are split.
        if let Some(broadcast) = &state.broadcast
            && ecx.journaled_state.depth() >= broadcast.depth
            && !state.config.evm_opts.isolate
        {
            foundry_zksync_core::increment_tx_nonce(broadcast.new_origin, ecx);
            debug!("incremented zksync nonce after broadcastable create");
        }
    }

    /// Persist factory deps to make them available at execution time.
    /// This might be necessary for any factory deps deployed with libraries.
    fn zksync_persist_factory_deps(
        &self,
        ctx: &mut dyn CheatcodeInspectorStrategyContext,
        factory_deps: HashMap<B256, Vec<u8>>,
    ) {
        let ctx = get_context(ctx);
        ctx.persisted_factory_deps
            .extend(factory_deps.into_iter().map(|(hash, bytecode)| (H256(hash.0), bytecode)));
    }
}

impl ZksyncCheatcodeInspectorStrategyRunner {
    /// Selects the appropriate VM for the fork. Options: EVM, ZK-VM.
    /// CALL and CREATE are handled by the selected VM.
    ///
    /// Additionally:
    /// * Translates block information
    /// * Translates all persisted addresses
    pub fn select_fork_vm(
        &self,
        ctx: &mut ZksyncCheatcodeInspectorStrategyContext,
        data: Ecx<'_, '_, '_>,
        fork_id: LocalForkId,
    ) {
        let fork_info =
            data.journaled_state.database.get_fork_info(fork_id).expect("failed getting fork info");
        if fork_info.fork_type.is_evm() {
            self.select_evm(ctx, data)
        } else {
            self.select_zk_vm(ctx, data, Some(&fork_info.fork_env))
        }
    }

    /// Switch to EVM and translate block info, balances, nonces and deployed codes for persistent
    /// accounts
    pub fn select_evm(
        &self,
        ctx: &mut ZksyncCheatcodeInspectorStrategyContext,
        data: Ecx<'_, '_, '_>,
    ) {
        if !ctx.using_zk_vm {
            tracing::info!("already in EVM");
            return;
        }

        tracing::info!("switching to EVM");
        ctx.using_zk_vm = false;

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
        let block_info =
            data.journaled_state.sload(system_account, block_info_key).unwrap_or_default();
        let (block_number, block_timestamp) = unpack_block_info(block_info.to_u256());
        data.block.number = U256::from(block_number);
        data.block.timestamp = U256::from(block_timestamp);

        let test_contract = data.journaled_state.database.get_test_contract_address();
        for address in
            data.journaled_state.database.persistent_accounts().into_iter().chain([data.tx.caller])
        {
            info!(?address, "importing to evm state");

            let balance_key = get_balance_key(address);
            let nonce_key = get_nonce_key(address);

            let balance =
                data.journaled_state.sload(balance_account, balance_key).unwrap_or_default().data;
            let full_nonce =
                data.journaled_state.sload(nonce_account, nonce_key).unwrap_or_default();
            let (tx_nonce, deployment_nonce) = decompose_full_nonce(full_nonce.to_u256());
            if !deployment_nonce.is_zero() {
                warn!(
                    ?address,
                    ?deployment_nonce,
                    "discarding ZKsync deployment nonce for EVM context, might cause inconsistencies"
                );
            }
            let nonce = tx_nonce.as_u64();

            let account_code_key = get_account_code_key(address);
            let (code_hash, code) = data
                .journaled_state
                .sload(account_code_account, account_code_key)
                .ok()
                .and_then(|zk_bytecode_hash| {
                    ctx.dual_compiled_contracts
                        .find_by_zk_bytecode_hash(zk_bytecode_hash.to_h256())
                        .map(|(_, contract)| {
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
                trace!(?address, "ignoring code translation for test contract");
            } else {
                account.info.code_hash = code_hash;
                account.info.code.clone_from(&code);
            }
        }
    }

    /// Switch to ZK-VM and translate block info, balances, nonces and deployed codes for persistent
    /// accounts
    pub fn select_zk_vm(
        &self,
        ctx: &mut ZksyncCheatcodeInspectorStrategyContext,
        data: Ecx<'_, '_, '_>,
        new_env: Option<&Env>,
    ) {
        if ctx.using_zk_vm {
            tracing::info!("already in ZK-VM");
            return;
        }

        tracing::info!("switching to ZK-VM");
        ctx.using_zk_vm = true;

        let block_env = match new_env {
            Some(env) => &env.evm_env.block_env,
            None => &data.block,
        };

        let mut system_storage: HashMap<U256, EvmStorageSlot> = Default::default();
        let block_info_key = CURRENT_VIRTUAL_BLOCK_INFO_POSITION.to_ru256();
        let block_info =
            pack_block_info(block_env.number.saturating_to(), block_env.timestamp.saturating_to());
        system_storage.insert(block_info_key, EvmStorageSlot::new(block_info.to_ru256(), 0));

        let mut l2_eth_storage: HashMap<U256, EvmStorageSlot> = Default::default();
        let mut nonce_storage: HashMap<U256, EvmStorageSlot> = Default::default();
        let mut account_code_storage: HashMap<U256, EvmStorageSlot> = Default::default();
        let mut known_codes_storage: HashMap<U256, EvmStorageSlot> = Default::default();
        let mut deployed_codes: HashMap<Address, AccountInfo> = Default::default();

        let test_contract = data.journaled_state.database.get_test_contract_address();

        for address in
            data.journaled_state.database.persistent_accounts().into_iter().chain([data.tx.caller])
        {
            info!(?address, "importing to zk state");

            // Reuse the deployment nonce from storage if present.
            let deployment_nonce = {
                let nonce_key = get_nonce_key(address);
                let nonce_addr = NONCE_HOLDER_ADDRESS.to_address();
                let account = journaled_account(data, nonce_addr).expect("failed to load account");
                if let Some(value) = account.storage.get(&nonce_key) {
                    let full_nonce = parse_full_nonce(value.original_value);
                    debug!(
                        ?address,
                        deployment_nonce = full_nonce.deploy_nonce,
                        "reuse existing deployment nonce"
                    );
                    full_nonce.deploy_nonce.into()
                } else {
                    zksync_types::U256::zero()
                }
            };

            let account = journaled_account(data, address).expect("failed to load account");
            let info = &account.info;

            let balance_key = get_balance_key(address);
            l2_eth_storage.insert(balance_key, EvmStorageSlot::new(info.balance, 0));

            debug!(?address, ?deployment_nonce, transaction_nonce=?info.nonce, "attempting to fit EVM nonce to ZKsync nonces, might cause inconsistencies");
            let full_nonce = nonces_to_full_nonce(info.nonce.into(), deployment_nonce);

            let nonce_key = get_nonce_key(address);
            nonce_storage.insert(nonce_key, EvmStorageSlot::new(full_nonce.to_ru256(), 0));

            if let Some(bytecode) = &info.code {
                // TODO(zk): This has O(N*M) complexity, since for each contract we need to
                // reset immutables in the deployed bytecode and compare it against dual compiled
                // contract. Given that we're already in the loop, it can get pretty
                // slow for big projects.
                if let Some((_, contract)) = ctx
                    .dual_compiled_contracts
                    .find_by_evm_deployed_bytecode_with_immutables(bytecode.original_byte_slice())
                {
                    account_code_storage.insert(
                        get_account_code_key(address),
                        EvmStorageSlot::new(contract.zk_bytecode_hash.to_ru256(), 0),
                    );
                    known_codes_storage.insert(
                        contract.zk_bytecode_hash.to_ru256(),
                        EvmStorageSlot::new(U256::ZERO, 0),
                    );

                    let code_hash = B256::from_slice(contract.zk_bytecode_hash.as_bytes());
                    if Some(address) != test_contract {
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
                        // We cannot override test contract info in the account, since calls to the
                        // test contract on a high level are processed in
                        // the EVM, so if we'll override the bytecode, we won't be able to
                        // execute it.
                        // However, we can set the account code hash in the `AccountCodeStorage` and
                        // persist the factory dep so that the code can be
                        // decommitted; this way the test contract can be invoked
                        // by other contracts from within EraVM.
                        // TODO(zk): Do we actually need to override code in accounts in general? It
                        // feels like relying _just_ on the
                        // `AccountCodeStorage` and factory deps would be enough.
                        ctx.persisted_factory_deps.insert(
                            contract.zk_bytecode_hash,
                            contract.zk_deployed_bytecode.clone(),
                        );

                        if contract
                            .evm_immutable_references
                            .as_ref()
                            .map(|refs| !refs.is_empty())
                            .unwrap_or(false)
                        {
                            // TODO(zk): Test contract is deployed in a special way, so we do not
                            // catch the immutables that were set
                            // for it. Based on the deployed bytecode itself we cannot calculate the
                            // right slots for `ImmutableSimulator`,
                            // as `zksolc` assigns slots in the order of immutable construction in
                            // Yul. It means that while we can migrate
                            // the test contract to the EraVM, it will have all the
                            // immutables set to `0x0`.
                            tracing::warn!(
                                ?address,
                                "test contract has immutables, but they are not set in the EraVM",
                            );
                        }
                    }
                    tracing::info!(
                        ?address,
                        code_hash = ?info.code_hash,
                        "found zk contract",
                    );
                } else {
                    tracing::info!(code_hash = ?info.code_hash, ?address, "no zk contract found")
                }
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

        for (address, info) in deployed_codes {
            let account = journaled_account(data, address).expect("failed to load account");
            let _ = std::mem::replace(&mut account.info.balance, info.balance);
            let _ = std::mem::replace(&mut account.info.nonce, info.nonce);
            account.info.code_hash = info.code_hash;
            account.info.code.clone_from(&info.code);
        }
    }
}

fn get_context(
    ctx: &mut dyn CheatcodeInspectorStrategyContext,
) -> &mut ZksyncCheatcodeInspectorStrategyContext {
    ctx.as_any_mut().downcast_mut().expect("expected ZksyncCheatcodeInspectorStrategyContext")
}
