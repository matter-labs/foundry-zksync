use std::sync::Arc;

use alloy_primitives::{map::HashMap, Address, Bytes, TxKind, B256, U256};
use alloy_rpc_types::{
    request::{TransactionInput, TransactionRequest},
    serde_helpers::WithOtherFields,
};
use foundry_cheatcodes::{
    journaled_account,
    strategy::{
        CheatcodeInspectorStrategyContext, CheatcodeInspectorStrategyExt,
        CheatcodeInspectorStrategyRunner, EvmCheatcodeInspectorStrategyRunner,
    },
    Broadcast, BroadcastableTransaction, BroadcastableTransactions, Cheatcodes, CheatcodesExecutor,
    CheatsConfig, CheatsCtxt, CommonCreateInput, DynCheatcode, Ecx, InnerEcx, Result,
    Vm::{self, AccountAccess, AccountAccessKind, ChainInfo, StorageAccess},
};
use foundry_common::TransactionMaybeSigned;
use foundry_evm::{
    backend::{DatabaseError, LocalForkId},
    constants::{DEFAULT_CREATE2_DEPLOYER, DEFAULT_CREATE2_DEPLOYER_CODE},
};
use foundry_evm_core::backend::DatabaseExt;
use foundry_zksync_core::{
    convert::{ConvertAddress, ConvertH160, ConvertH256, ConvertRU256, ConvertU256},
    get_account_code_key, get_balance_key, get_nonce_key,
    state::parse_full_nonce,
    PaymasterParams, ZkTransactionMetadata, ACCOUNT_CODE_STORAGE_ADDRESS,
    CONTRACT_DEPLOYER_ADDRESS, DEFAULT_CREATE2_DEPLOYER_ZKSYNC, KNOWN_CODES_STORAGE_ADDRESS,
    L2_BASE_TOKEN_ADDRESS, NONCE_HOLDER_ADDRESS, ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY,
};
use itertools::Itertools;
use revm::{
    interpreter::{
        opcode as op, CallInputs, CallOutcome, CreateOutcome, Gas, InstructionResult, Interpreter,
        InterpreterResult,
    },
    primitives::{
        AccountInfo, Bytecode, CreateScheme, Env, EvmStorageSlot, ExecutionResult, HashSet, Output,
        SignedAuthorization, KECCAK_EMPTY,
    },
};
use tracing::{debug, error, info, trace, warn};
use zksync_types::{
    block::{pack_block_info, unpack_block_info},
    utils::{decompose_full_nonce, nonces_to_full_nonce},
    CURRENT_VIRTUAL_BLOCK_INFO_POSITION, SYSTEM_CONTEXT_ADDRESS,
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
            // call/create end. We have no easy way to skip that logic as of now, so
            // we record the index the duplicate entry will be at and remove it
            // via call to`zksync_fix_recorded_acceses`.
            //
            // TODO(zk): This is currently a hack, as account access recording is
            // done in 4 parts - create/create_end and call/call_end. And these must all be
            // moved to strategy.
            //
            // If we have a pending stack, it will be appended to the end of primary stack, else at
            // the beginning, once the record is finalized.
            let stack_insert_index = if recorded_account_diffs_stack.len() > 1 {
                recorded_account_diffs_stack.first().map_or(0, Vec::len)
            } else {
                0
            };

            if let Some(last) = recorded_account_diffs_stack.last_mut() {
                let ctx = get_context(state.strategy.context.as_mut());
                ctx.remove_recorded_access_at = Some(stack_insert_index);

                for record in account_accesses {
                    let access = AccountAccess {
                        chainInfo: ChainInfo {
                            forkId: ecx.db.active_fork_id().unwrap_or_default(),
                            chainId: U256::from(ecx.env.cfg.chain_id),
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
        ecx_inner: InnerEcx<'_, '_, '_>,
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

        let is_fixed_gas_limit =
            foundry_cheatcodes::check_if_fixed_gas_limit(ecx_inner, input.gas_limit());

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

        let rpc = ecx_inner.db.active_fork_url();

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
            gas: if is_fixed_gas_limit { Some(input.gas_limit()) } else { None },
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
        ecx_inner: InnerEcx<'_, '_, '_>,
        broadcast: &Broadcast,
        broadcastable_transactions: &mut BroadcastableTransactions,
        active_delegation: &mut Option<SignedAuthorization>,
    ) {
        let ctx_zk = get_context(ctx);

        if !ctx_zk.using_zk_vm {
            return EvmCheatcodeInspectorStrategyRunner.record_broadcastable_call_transactions(
                ctx,
                config,
                call,
                ecx_inner,
                broadcast,
                broadcastable_transactions,
                active_delegation,
            );
        }

        let ctx = ctx_zk;

        let is_fixed_gas_limit =
            foundry_cheatcodes::check_if_fixed_gas_limit(ecx_inner, call.gas_limit);

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
            input: TransactionInput::new(call.input.clone()),
            nonce: Some(tx_nonce as u64),
            chain_id: Some(ecx_inner.env.cfg.chain_id),
            gas: if is_fixed_gas_limit { Some(call.gas_limit) } else { None },
            ..Default::default()
        };

        if let Some(auth_list) = active_delegation.take() {
            tx_req.authorization_list = Some(vec![auth_list]);
        } else {
            tx_req.authorization_list = None;
        }
        let mut tx = WithOtherFields::new(tx_req);

        tx.other.insert(
            ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY.to_string(),
            serde_json::to_value(zk_tx).expect("failed encoding json"),
        );

        broadcastable_transactions.push_back(BroadcastableTransaction {
            rpc: ecx_inner.db.active_fork_url(),
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

        let address = match interpreter.current_opcode() {
            op::SELFBALANCE => interpreter.contract().target_address,
            op::BALANCE => {
                if interpreter.stack.is_empty() {
                    interpreter.instruction_result = InstructionResult::StackUnderflow;
                    return true;
                }

                Address::from_word(B256::from(unsafe { interpreter.stack.pop_unsafe() }))
            }
            _ => return true,
        };

        // Safety: Length is checked above.
        let balance = foundry_zksync_core::balance(address, ecx);

        // Skip the current BALANCE instruction since we've already handled it
        match interpreter.stack.push(balance) {
            Ok(_) => unsafe {
                interpreter.instruction_pointer = interpreter.instruction_pointer.add(1);
            },
            Err(e) => {
                interpreter.instruction_result = e;
            }
        };

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
        ctx: &mut dyn CheatcodeInspectorStrategyContext,
        call: &mut CallInputs,
    ) {
        let ctx = get_context(ctx);

        ctx.set_deployer_call_input_factory_deps.clear();
        if call.target_address == DEFAULT_CREATE2_DEPLOYER && ctx.using_zk_vm {
            call.target_address = DEFAULT_CREATE2_DEPLOYER_ZKSYNC;
            call.bytecode_address = DEFAULT_CREATE2_DEPLOYER_ZKSYNC;

            let (salt, init_code) = call.input.split_at(32);
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

            call.input = create_input.into();
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

        if let Some(CreateScheme::Create) = input.scheme() {
            let caller = input.caller();
            let nonce = ecx
                .inner
                .journaled_state
                .load_account(input.caller(), &mut ecx.inner.db)
                .expect("to load caller account")
                .info
                .nonce;
            let address = caller.create(nonce);
            if ecx.db.get_test_contract_address().map(|addr| address == addr).unwrap_or_default() {
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
        let injected_factory_deps = ctx
            .zk_use_factory_deps
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
                let res = ctx.dual_compiled_contracts.find_bytecode(&artifact_code).unwrap();
                ctx.dual_compiled_contracts.fetch_all_factory_deps(res.contract())
            })
            .collect_vec();
        factory_deps.extend(injected_factory_deps);

        // NOTE(zk): Clear injected factory deps so that they are not sent on further transactions
        ctx.zk_use_factory_deps.clear();
        tracing::debug!(contract = info.name, "using dual compiled contract");

        let ccx = foundry_zksync_core::vm::CheatcodeTracerContext {
            mocked_calls: state.mocked_calls.clone(),
            expected_calls: Some(&mut state.expected_calls),
            accesses: state.accesses.as_mut(),
            persisted_factory_deps: Some(&mut ctx.persisted_factory_deps),
            paymaster_data: ctx.paymaster_params.take(),
            zk_env: ctx.zk_env.clone(),
            record_storage_accesses: state.recorded_account_diffs_stack.is_some(),
        };

        let zk_create = foundry_zksync_core::vm::ZkCreateInputs {
            value: input.value().to_u256(),
            msg_sender: input.caller(),
            create_input: zk_create_input,
            factory_deps,
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
                                ecx: &mut ecx.inner,
                                precompiles: &mut ecx.precompiles,
                                gas_limit: input.gas_limit(),
                                caller: input.caller(),
                            },
                            &decoded_log,
                        );
                    },
                );

                // append traces
                executor.trace_zksync(state, ecx, result.call_traces);

                // for each log in cloned logs call handle_expect_emit
                if !state.expected_emits.is_empty() {
                    for log in result.logs {
                        foundry_cheatcodes::handle_expect_emit(
                            state,
                            &log,
                            &mut Default::default(),
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
                        let strategy = ecx.db.get_strategy();
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
            .db
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
            accesses: state.accesses.as_mut(),
            persisted_factory_deps: Some(&mut ctx.persisted_factory_deps),
            paymaster_data: ctx.paymaster_params.take(),
            zk_env: ctx.zk_env.clone(),
            record_storage_accesses: state.recorded_account_diffs_stack.is_some(),
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
                                ecx: &mut ecx.inner,
                                precompiles: &mut ecx.precompiles,
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
                    executor.trace_zksync(state, ecx, result.call_traces);

                    // for each log in cloned logs call handle_expect_emit
                    if !state.expected_emits.is_empty() {
                        for log in result.logs {
                            foundry_cheatcodes::handle_expect_emit(
                                state,
                                &log,
                                &mut Default::default(),
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
                            }),
                            _ => Some(CallOutcome {
                                result: InterpreterResult {
                                    result: InstructionResult::Revert,
                                    output: Bytes::new(),
                                    gas,
                                },
                                memory_offset: call.return_memory_offset.clone(),
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
                        })
                    }
                    ExecutionResult::Halt { .. } => Some(CallOutcome {
                        result: InterpreterResult {
                            result: InstructionResult::Revert,
                            output: Bytes::from_iter(String::from("zk vm halted").as_bytes()),
                            gas,
                        },
                        memory_offset: call.return_memory_offset.clone(),
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
                })
            }
        }
    }

    fn zksync_remove_duplicate_account_access(&self, state: &mut Cheatcodes) {
        let ctx = get_context(state.strategy.context.as_mut());

        if let Some(index) = ctx.remove_recorded_access_at.take() {
            if let Some(recorded_account_diffs_stack) = state.recorded_account_diffs_stack.as_mut()
            {
                if let Some(last) = recorded_account_diffs_stack.first_mut() {
                    // This entry has been inserted during CREATE/CALL operations in revm's
                    // cheatcode inspector and must be removed.
                    let _ = last.remove(index);
                }
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
        if let Some(broadcast) = &state.broadcast {
            if ecx.inner.journaled_state.depth() >= broadcast.depth &&
                !state.config.evm_opts.isolate
            {
                foundry_zksync_core::increment_tx_nonce(broadcast.new_origin, &mut ecx.inner);
                debug!("incremented zksync nonce after broadcastable create");
            }
        }
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
        data: InnerEcx<'_, '_, '_>,
        fork_id: LocalForkId,
    ) {
        let fork_info = data.db.get_fork_info(fork_id).expect("failed getting fork info");
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
        data: InnerEcx<'_, '_, '_>,
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
        let block_info = data.sload(system_account, block_info_key).unwrap_or_default();
        let (block_number, block_timestamp) = unpack_block_info(block_info.to_u256());
        data.env.block.number = U256::from(block_number);
        data.env.block.timestamp = U256::from(block_timestamp);

        let test_contract = data.db.get_test_contract_address();
        for address in data.db.persistent_accounts().into_iter().chain([data.env.tx.caller]) {
            info!(?address, "importing to evm state");

            let balance_key = get_balance_key(address);
            let nonce_key = get_nonce_key(address);

            let balance = data.sload(balance_account, balance_key).unwrap_or_default().data;
            let full_nonce = data.sload(nonce_account, nonce_key).unwrap_or_default();
            let (tx_nonce, deployment_nonce) = decompose_full_nonce(full_nonce.to_u256());
            if !deployment_nonce.is_zero() {
                warn!(?address, ?deployment_nonce, "discarding ZKsync deployment nonce for EVM context, might cause inconsistencies");
            }
            let nonce = tx_nonce.as_u64();

            let account_code_key = get_account_code_key(address);
            let (code_hash, code) = data
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
        data: InnerEcx<'_, '_, '_>,
        new_env: Option<&Env>,
    ) {
        if ctx.using_zk_vm {
            tracing::info!("already in ZK-VM");
            return;
        }

        tracing::info!("switching to ZK-VM");
        ctx.using_zk_vm = true;

        let env = new_env.unwrap_or(data.env.as_ref());

        let mut system_storage: HashMap<U256, EvmStorageSlot> = Default::default();
        let block_info_key = CURRENT_VIRTUAL_BLOCK_INFO_POSITION.to_ru256();
        let block_info =
            pack_block_info(env.block.number.as_limbs()[0], env.block.timestamp.as_limbs()[0]);
        system_storage.insert(block_info_key, EvmStorageSlot::new(block_info.to_ru256()));

        let mut l2_eth_storage: HashMap<U256, EvmStorageSlot> = Default::default();
        let mut nonce_storage: HashMap<U256, EvmStorageSlot> = Default::default();
        let mut account_code_storage: HashMap<U256, EvmStorageSlot> = Default::default();
        let mut known_codes_storage: HashMap<U256, EvmStorageSlot> = Default::default();
        let mut deployed_codes: HashMap<Address, AccountInfo> = Default::default();

        let test_contract = data.db.get_test_contract_address();
        for address in data.db.persistent_accounts().into_iter().chain([data.env.tx.caller]) {
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
            l2_eth_storage.insert(balance_key, EvmStorageSlot::new(info.balance));

            debug!(?address, ?deployment_nonce, transaction_nonce=?info.nonce, "attempting to fit EVM nonce to ZKsync nonces, might cause inconsistencies");
            let full_nonce = nonces_to_full_nonce(info.nonce.into(), deployment_nonce);

            let nonce_key = get_nonce_key(address);
            nonce_storage.insert(nonce_key, EvmStorageSlot::new(full_nonce.to_ru256()));

            if test_contract.map(|test_address| address == test_address).unwrap_or_default() {
                // avoid migrating test contract code
                trace!(?address, "ignoring code translation for test contract");
                continue;
            }

            if let Some((_, contract)) = ctx.dual_compiled_contracts.iter().find(|(_, contract)| {
                info.code_hash != KECCAK_EMPTY && info.code_hash == contract.evm_bytecode_hash
            }) {
                account_code_storage.insert(
                    get_account_code_key(address),
                    EvmStorageSlot::new(contract.zk_bytecode_hash.to_ru256()),
                );
                known_codes_storage
                    .insert(contract.zk_bytecode_hash.to_ru256(), EvmStorageSlot::new(U256::ZERO));

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
                tracing::debug!(code_hash = ?info.code_hash, ?address, "no zk contract found")
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
