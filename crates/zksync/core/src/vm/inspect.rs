use alloy_evm::eth::EthEvmContext;
use alloy_primitives::{FixedBytes, Log, hex};
use anvil_zksync_config::types::SystemContractsOptions as Options;
use anvil_zksync_core::{
    formatter::transaction::summary::TransactionSummary, system_contracts::SystemContracts,
};
use anvil_zksync_traces::{
    build_call_trace_arena, decode_trace_arena, filter_call_trace_arena,
    identifier::SignaturesIdentifier, render_trace_arena_inner,
};
use core::convert::Into;
use foundry_common::sh_println;
use itertools::Itertools;
use revm::{
    Database,
    bytecode::Bytecode,
    context::{
        JournalTr,
        result::{
            ExecutionResult as rExecutionResult, HaltReason, OutOfGasError, Output, SuccessReason,
        },
    },
    database::states::StorageSlot,
    primitives::{Address, B256, Bytes, HashMap as rHashMap, Log as rLog, U256 as rU256},
};
use std::{
    collections::HashMap,
    fmt::Debug,
    sync::{Arc, LazyLock, RwLock},
};
use tracing::{debug, error, info, trace, warn};
use zksync_basic_types::{H160, H256, L2ChainId, Nonce, U256, ethabi};
use zksync_multivm::{
    interface::{
        Call, CallType, ExecutionResult, Halt, InspectExecutionMode, VmEvent,
        VmExecutionResultAndLogs, VmFactory, VmInterface, VmRevertReason,
    },
    tracers::CallTracer,
    vm_latest::{HistoryDisabled, ToTracerPointer, Vm},
};
use zksync_types::{
    ACCOUNT_CODE_STORAGE_ADDRESS, CONTRACT_DEPLOYER_ADDRESS, PackedEthSignature, StorageKey,
    Transaction, bytecode::BytecodeHash, get_nonce_key, h256_to_address, l2::L2Tx,
    transaction_request::PaymasterParams,
};
use zksync_vm_interface::storage::{ReadStorage, StoragePtr, WriteStorage};

use crate::{
    DEFAULT_PROTOCOL_VERSION, MAX_L2_GAS_LIMIT,
    convert::{ConvertAddress, ConvertH160, ConvertH256, ConvertRU256},
    fix_l2_gas_limit, fix_l2_gas_price, increment_tx_nonce, is_system_address,
    state::{FullNonce, new_full_nonce, parse_full_nonce},
    vm::{
        db::{DEFAULT_CHAIN_ID, ZKVMData},
        decoder::CallTraceDecoderBuilder,
        env::{create_l1_batch_env, create_system_env},
        storage_recorder::{AccountAccess, StorageAccessRecorder},
        storage_view::StorageView,
        tracers::{
            bootloader::{BootloaderDebug, BootloaderDebugTracer},
            cheatcode::{CallContext, CheatcodeTracer, CheatcodeTracerContext},
            error::ErrorTracer,
        },
    },
};

use foundry_evm_abi::console::{self, ds::Console};

use super::HARDHAT_CONSOLE_ADDRESS;

/// Represents the result of execution a [`L2Tx`] on EraVM
#[derive(Debug)]
pub struct ZKVMExecutionResult {
    /// The logs of a given execution
    pub logs: Vec<rLog>,
    /// The result of a given execution
    pub execution_result: rExecutionResult,
    /// Call traces
    pub call_traces: Vec<Call>,
    /// Immutables recorded via calls to ImmutableSimulator::setImmutables.
    pub recorded_immutables: rHashMap<H160, rHashMap<rU256, FixedBytes<32>>>,
    /// Recorded account accesses.
    pub account_accesses: Vec<AccountAccess>,
}

/// Revm-style result with ZKVM Execution
pub type ZKVMResult<E> = Result<ZKVMExecutionResult, E>;

/// Same as [`inspect`] but batches factory deps to account for size limitations.
///
/// Will handle aggregating execution results, where errors, reverts or halts will be propagated
/// immediately.
/// All logs will be collected as they happen, and returned with the final result.
// TODO: should we make this transparent in `inspect` directly?
pub fn inspect_as_batch<DB, E>(
    tx: L2Tx,
    ecx: &mut EthEvmContext<DB>,
    ccx: &mut CheatcodeTracerContext,
    call_ctx: CallContext,
) -> ZKVMResult<E>
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    let txns = split_tx_by_factory_deps(tx);
    let total_txns = txns.len();
    let mut aggregated_result: Option<ZKVMExecutionResult> = None;

    for (idx, mut tx) in txns.into_iter().enumerate() {
        let pending_txs = total_txns - idx;

        // cap gas limit so that we do not set a number greater than
        // remaining sender balance
        let (new_gas_limit, _) = gas_params(
            ecx,
            tx.common_data.initiator_address.to_address(),
            &tx.common_data.paymaster_params,
        );
        tx.common_data.fee.gas_limit = new_gas_limit;

        let initiator_address = tx.initiator_account();
        info!("executing batched tx ({}/{})", idx + 1, total_txns);
        let mut result = inspect(tx, ecx, ccx, call_ctx.clone())?;

        match (&mut aggregated_result, result.execution_result) {
            (_, exec @ rExecutionResult::Revert { .. } | exec @ rExecutionResult::Halt { .. }) => {
                return Ok(ZKVMExecutionResult {
                    logs: result.logs,
                    call_traces: result.call_traces,
                    execution_result: exec,
                    recorded_immutables: result.recorded_immutables,
                    account_accesses: result.account_accesses,
                });
            }
            (None, exec) => {
                aggregated_result.replace(ZKVMExecutionResult {
                    logs: result.logs,
                    call_traces: result.call_traces,
                    execution_result: exec,
                    recorded_immutables: result.recorded_immutables,
                    account_accesses: result.account_accesses,
                });
            }
            (Some(zk_result), reth_result) => {
                assert!(
                    matches!(reth_result, rExecutionResult::Success { .. }),
                    "aggregated result must only contain success, got: {reth_result:?}"
                );
                zk_result.logs.append(&mut result.logs);
                zk_result.call_traces.append(&mut result.call_traces);
                zk_result.recorded_immutables.extend(result.recorded_immutables);
                zk_result.account_accesses.extend(result.account_accesses);
                zk_result.execution_result = reth_result;
            }
        }

        // Increment the nonce manually if there are other transactions
        // to be executed after the current one
        if pending_txs > 1 {
            increment_tx_nonce(initiator_address.to_address(), ecx);
        }
    }

    Ok(aggregated_result.expect("must have result"))
}

/// Processes a [`L2Tx`] with EraVM and returns the final execution result and logs.
///
/// State changes will be reflected in the given `Env`, `DB`, `JournaledState`.
pub fn inspect<DB, E>(
    mut tx: L2Tx,
    ecx: &mut EthEvmContext<DB>,
    ccx: &mut CheatcodeTracerContext,
    call_ctx: CallContext,
) -> ZKVMResult<E>
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    let chain_id = if ecx.cfg.chain_id <= u32::MAX as u64 {
        L2ChainId::from(ecx.cfg.chain_id as u32)
    } else {
        warn!(provided = ?ecx.cfg.chain_id, using = DEFAULT_CHAIN_ID, "using default chain id as provided chain_id does not fit into u32");
        L2ChainId::from(DEFAULT_CHAIN_ID)
    };

    let persisted_factory_deps = ccx
        .persisted_factory_deps
        .as_ref()
        .map(|factory_deps| (*factory_deps).clone())
        .unwrap_or_default();

    let mut era_db = ZKVMData::new_with_system_contracts(ecx, chain_id)
        .with_extra_factory_deps(persisted_factory_deps)
        .with_storage_accesses(ccx.accesses.take());

    info!(?call_ctx, "executing transaction in zk vm");

    let initiator_address = tx.common_data.initiator_address;

    if tx.common_data.signature.is_empty() {
        // FIXME: This is a hack to make sure that the signature is not empty.
        // Fails without a signature here: https://github.com/matter-labs/zksync-era/blob/73a1e8ff564025d06e02c2689da238ae47bb10c3/core/lib/types/src/transaction_request.rs#L381
        tx.common_data.signature = PackedEthSignature::default().serialize_packed().into();
    }

    let modified_storage_keys = era_db.override_keys.clone();
    let storage_ptr =
        StorageView::new(&mut era_db, modified_storage_keys, tx.common_data.initiator_address)
            .into_rc_ptr();
    let InnerZkVmResult {
        tx_result,
        bytecodes,
        mut modified_storage,
        call_traces,
        recorded_immutables,
        create_outcome,
        gas_usage,
    } = inspect_inner(tx, storage_ptr, chain_id, ccx, call_ctx.clone());

    info!(
        reserved=?gas_usage.bootloader_debug.reserved_gas, limit=?gas_usage.limit, execution=?gas_usage.execution, pubdata=?gas_usage.pubdata, refunded=?gas_usage.refunded,
        "gas usage",
    );

    let account_accesses = era_db.get_account_accesses();

    // TODO(zk): adapt this to use account_accesses as well
    if let Some(record) = &mut era_db.accesses {
        for k in modified_storage.keys() {
            record.writes.entry(k.address().to_address()).or_default().push(k.key().to_ru256());
        }
    }

    let logs = tx_result
        .logs
        .events
        .clone()
        .into_iter()
        .map(|event| {
            revm::primitives::Log::new_unchecked(
                event.address.to_address(),
                event.indexed_topics.iter().cloned().map(|t| B256::from(t.0)).collect(),
                event.value.into(),
            )
        })
        .collect_vec();

    let execution_result = match tx_result.result {
        ExecutionResult::Success { output, .. } => {
            let result = ethabi::decode(&[ethabi::ParamType::Bytes], &output)
                .ok()
                .and_then(|result| result.first().cloned())
                .and_then(|result| result.into_bytes())
                .unwrap_or_default();
            info!("zk vm decoded result {}", hex::encode(&result));

            let address = if result.len() == 32 {
                Some(h256_to_address(&H256::from_slice(&result)))
            } else {
                None
            };
            // in zkEVM the output is the 0-padded address, we replace this with the deployed
            // bytecode so the traces can pick it up correctly
            let output = if call_ctx.is_create {
                let create_result = match (address, create_outcome) {
                    (Some(address), Some(create_outcome)) => {
                        if address == create_outcome.address {
                            create_outcome.bytecode
                        } else {
                            result
                        }
                    }
                    _ => result,
                };
                Output::Create(Bytes::from(create_result), address.map(ConvertH160::to_address))
            } else {
                Output::Call(Bytes::from(result))
            };

            ZKVMExecutionResult {
                logs: logs.clone(),
                call_traces,
                execution_result: rExecutionResult::Success {
                    reason: SuccessReason::Return,
                    gas_used: gas_usage.gas_used(),
                    gas_refunded: tx_result.refunds.gas_refunded,
                    logs,
                    output,
                },
                recorded_immutables,
                account_accesses,
            }
        }
        ExecutionResult::Revert { output } => {
            let output = match output {
                VmRevertReason::General { data, .. } => data,
                VmRevertReason::Unknown { data, .. } => data,
                _ => Vec::new(),
            };

            ZKVMExecutionResult {
                logs,
                call_traces,
                execution_result: rExecutionResult::Revert {
                    gas_used: gas_usage.gas_used(),
                    output: Bytes::from(output),
                },
                recorded_immutables,
                account_accesses,
            }
        }
        ExecutionResult::Halt { reason } => {
            error!("tx execution halted: {}", reason);
            let mapped_reason = match reason {
                Halt::NotEnoughGasProvided => HaltReason::OutOfGas(OutOfGasError::Basic),
                _ => HaltReason::PrecompileError,
            };

            ZKVMExecutionResult {
                logs,
                call_traces,
                execution_result: rExecutionResult::Halt {
                    reason: mapped_reason,
                    gas_used: gas_usage.gas_used(),
                },
                recorded_immutables,
                account_accesses,
            }
        }
    };

    // Insert into persisted_bytecodes. This is currently used in
    // deploying multiple factory_dep transactions in create to ensure
    // create does not OOG (Out of Gas) due to large factory deps.
    if let Some(persisted_factory_deps) = ccx.persisted_factory_deps.as_mut() {
        for (hash, bytecode) in &bytecodes {
            persisted_factory_deps.insert(*hash, bytecode.clone());
        }
    }

    let mut storage: rHashMap<Address, rHashMap<rU256, StorageSlot>> = Default::default();
    let mut codes: rHashMap<Address, (B256, Bytecode)> = Default::default();

    let initiator_nonce_key = get_nonce_key(&initiator_address);

    // NOTE(zk): We need to revert the tx nonce of the initiator as we intercept and dispatch
    // CALLs and CREATEs to the zkEVM. The CREATEs always increment the deployment nonce
    // which must be persisted, but the tx nonce increase must be reverted.
    if let Some(initiator_nonce) = modified_storage.get_mut(&initiator_nonce_key) {
        let FullNonce { tx_nonce, deploy_nonce } = parse_full_nonce(initiator_nonce.to_ru256());
        let new_tx_nonce = tx_nonce.saturating_sub(1);
        trace!(address=?initiator_address, from=?tx_nonce, to=?new_tx_nonce, deploy_nonce, "reverting initiator tx nonce for CALL");
        *initiator_nonce = new_full_nonce(new_tx_nonce, deploy_nonce).to_h256();
    }

    for (k, v) in modified_storage {
        let address = k.address().to_address();
        let index = k.key().to_ru256();
        era_db.load_account(address);
        let previous = era_db.sload(address, index);
        let entry = storage.entry(address).or_default();

        entry.insert(index, StorageSlot::new_changed(previous, v.to_ru256()));

        if k.address() == &ACCOUNT_CODE_STORAGE_ADDRESS {
            if let Some(bytecode) = bytecodes.get(&v) {
                let bytecode = Bytecode::new_raw(Bytes::from(bytecode.clone()));
                let hash = B256::from_slice(v.as_bytes());
                codes.insert(k.key().to_h160().to_address(), (hash, bytecode));
            } else {
                // We populate bytecodes for all non-system addresses
                if !is_system_address(k.key().to_h160().to_address()) {
                    if let Some(bytecode) = (&mut era_db).load_factory_dep(v) {
                        let hash = B256::from_slice(v.as_bytes());
                        let bytecode = Bytecode::new_raw(Bytes::from(bytecode));
                        codes.insert(k.key().to_h160().to_address(), (hash, bytecode));
                    } else {
                        warn!(
                            "no bytecode was found for {:?}, requested by account {:?}",
                            v,
                            k.key().to_h160()
                        );
                    }
                }
            }
        }
    }

    for (address, storage) in storage {
        ecx.journaled_state.load_account(address).expect("account could not be loaded");
        ecx.journaled_state.touch(address);

        for (key, value) in storage {
            ecx.journaled_state
                .sstore(address, key, value.present_value)
                .expect("failed writing to slot");
        }
    }

    for (address, (code_hash, code)) in codes {
        ecx.journaled_state.load_account(address).expect("account could not be loaded");
        ecx.journaled_state.touch(address);
        let account = ecx.journaled_state.state.get_mut(&address).expect("account is loaded");

        account.info.code_hash = code_hash;
        account.info.code = Some(code);
    }

    Ok(execution_result)
}

/// Assign gas parameters that satisfy zkSync's fee model.
pub fn gas_params<DB>(
    ecx: &mut EthEvmContext<DB>,
    caller: Address,
    paymaster_params: &PaymasterParams,
) -> (U256, U256)
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    let value = ecx.tx.value.to_u256();
    let gas_price = U256::from(ecx.tx.gas_price);
    let gas_limit = U256::from(ecx.tx.gas_limit);

    let dev_mode = gas_price.is_zero();
    if !dev_mode {
        // return the original gas limit (subject to max tx gas limit in zkEVM) and gas price. If
        // not limited, causes not enough funds error in bootloader.
        let gas_limit = gas_limit.min(MAX_L2_GAS_LIMIT.into());
        return (gas_limit, gas_price);
    }

    let use_paymaster = !paymaster_params.paymaster.is_zero();
    let payer = if use_paymaster {
        Address::from_slice(paymaster_params.paymaster.as_bytes())
    } else {
        caller
    };
    let balance = ZKVMData::new(ecx).get_balance(payer);

    let max_fee_per_gas = fix_l2_gas_price(gas_price);
    let gas_limit = fix_l2_gas_limit(gas_limit, max_fee_per_gas, value, balance);

    (gas_limit, max_fee_per_gas)
}

#[allow(dead_code)]
struct InnerCreateOutcome {
    address: H160,
    hash: H256,
    bytecode: Vec<u8>,
}

#[allow(unused)]
#[derive(Debug)]
struct ZkVmGasUsage {
    /// Gas limit set for the user excluding the reserved gas.
    pub limit: U256,
    /// Gas refunded after transaction execution by the operator.
    pub refunded: U256,
    /// Gas used for only on transaction execution (validation and execution).
    pub execution: U256,
    /// Gas used for publishing pubdata.
    pub pubdata: U256,
    /// Additional bootloader debug info for gas usage.
    pub bootloader_debug: BootloaderDebug,
}

impl ZkVmGasUsage {
    pub fn gas_used(&self) -> u64 {
        // Gas limit is capped by the environment so gas should never reach max u64
        self.execution.saturating_add(self.pubdata).as_u64()
    }
}

struct InnerZkVmResult {
    tx_result: VmExecutionResultAndLogs,
    bytecodes: HashMap<H256, Vec<u8>>,
    modified_storage: HashMap<StorageKey, H256>,
    call_traces: Vec<Call>,
    create_outcome: Option<InnerCreateOutcome>,
    gas_usage: ZkVmGasUsage,
    recorded_immutables: rHashMap<H160, rHashMap<rU256, FixedBytes<32>>>,
}

static BASELINE_CONTRACTS: LazyLock<zksync_contracts::BaseSystemContracts> = LazyLock::new(|| {
    SystemContracts::from_options(
        Options::BuiltInWithoutSecurity,
        None,
        DEFAULT_PROTOCOL_VERSION,
        true,
        Default::default(),
    )
    .contracts(zksync_vm_interface::TxExecutionMode::VerifyExecute, false)
    .clone()
});

fn inspect_inner<S: ReadStorage + StorageAccessRecorder>(
    l2_tx: L2Tx,
    storage: StoragePtr<StorageView<S>>,
    chain_id: L2ChainId,
    ccx: &mut CheatcodeTracerContext,
    call_ctx: CallContext,
) -> InnerZkVmResult {
    let batch_env = create_l1_batch_env(storage.clone(), &ccx.zk_env);
    let l2_gas_price = batch_env.fee_input.fair_l2_gas_price();

    let system_env = create_system_env(BASELINE_CONTRACTS.clone(), chain_id);

    let mut vm: Vm<_, HistoryDisabled> = Vm::new(batch_env, system_env, storage.clone());

    let tx: Transaction = l2_tx.into();

    let call_tracer_result = Arc::default();
    let cheatcode_tracer_result = Arc::default();
    let mut expected_calls = HashMap::<_, _>::new();
    if let Some(ec) = &ccx.expected_calls {
        for (addr, v) in ec.iter() {
            expected_calls.insert(*addr, v.clone());
        }
    }
    let is_static = call_ctx.is_static;
    let is_create = call_ctx.is_create;
    let bootloader_debug_tracer_result = Arc::new(RwLock::new(Err("result uninitialized".into())));
    let tracers = vec![
        ErrorTracer.into_tracer_pointer(),
        CallTracer::new(Arc::clone(&call_tracer_result)).into_tracer_pointer(),
        BootloaderDebugTracer { result: Arc::clone(&bootloader_debug_tracer_result) }
            .into_tracer_pointer(),
        CheatcodeTracer::new(
            ccx.mocked_calls.clone(),
            expected_calls,
            Arc::clone(&cheatcode_tracer_result),
            call_ctx,
        )
        .into_tracer_pointer(),
    ];
    let compressed_bytecodes = vm.push_transaction(tx.clone()).compressed_bytecodes.into_owned();
    let mut tx_result = vm.inspect(&mut tracers.into(), InspectExecutionMode::OneTx);

    let mut call_traces = Arc::try_unwrap(call_tracer_result).unwrap().take().unwrap_or_default();
    trace!(?tx_result.result, "zk vm result");

    match &tx_result.result {
        ExecutionResult::Success { output } => {
            debug!(output = hex::encode(output), "Call: Successful");
        }
        ExecutionResult::Revert { output } => {
            debug!(?output, "Call: Reverted");
        }
        ExecutionResult::Halt { reason } => {
            debug!(?reason, "Call: Halted");
        }
    };

    // update expected calls from cheatcode tracer's result
    let cheatcode_result =
        Arc::try_unwrap(cheatcode_tracer_result).unwrap().take().unwrap_or_default();
    if let Some(expected_calls) = ccx.expected_calls.as_mut() {
        expected_calls.extend(cheatcode_result.expected_calls);
    }
    let recorded_immutables = cheatcode_result.recorded_immutables;

    // populate gas usage info
    let bootloader_debug = Arc::try_unwrap(bootloader_debug_tracer_result)
        .unwrap()
        .into_inner()
        .unwrap()
        .expect("failed obtaining bootloader debug info");
    trace!("{bootloader_debug:?}");

    let total_gas_limit = bootloader_debug.total_gas_limit_from_user;
    let intrinsic_gas = total_gas_limit - bootloader_debug.gas_limit_after_intrinsic;
    let gas_for_validation =
        bootloader_debug.gas_limit_after_intrinsic - bootloader_debug.gas_after_validation;
    let gas_used_tx_execution =
        intrinsic_gas + gas_for_validation + bootloader_debug.gas_spent_on_execution;

    let gas_used_pubdata = bootloader_debug
        .gas_per_pubdata
        .saturating_mul(tx_result.statistics.pubdata_published.into());

    let gas_usage = ZkVmGasUsage {
        limit: total_gas_limit,
        execution: gas_used_tx_execution,
        pubdata: gas_used_pubdata,
        refunded: bootloader_debug.refund_by_operator,
        bootloader_debug,
    };

    let deployed_bytecode_hashes = tx_result
        .logs
        .events
        .iter()
        .filter(|event| event.address == CONTRACT_DEPLOYER_ADDRESS)
        .map(|event| {
            (
                event.indexed_topics.get(3).cloned().unwrap_or_default().to_h160(),
                event.indexed_topics.get(2).cloned().unwrap_or_default(),
            )
        })
        .collect();

    let log_parser = ConsoleLogParser::new();
    let console_logs = log_parser.get_logs(&call_traces, true);

    for log in console_logs {
        tx_result.logs.events.push(VmEvent {
            location: Default::default(),
            address: H160::zero(),
            indexed_topics: log.topics().iter().map(|topic| H256::from(topic.0)).collect(),
            value: log.data.data.to_vec(),
        });
    }

    if tracing::enabled!(target: "anvil_zksync_core::formatter", tracing::Level::INFO) {
        let mut tx = tx.clone();
        // TODO: This is a hack to be able to print traces on demand. If `tx_common.input` is not
        // set, getting tx hash will panic.
        // It will not print correct transaction hash though.
        if let zksync_types::ExecuteTransactionCommon::L2(tx_common) = &mut tx.common_data {
            if tx_common.input.is_none() {
                tx_common.input = Some(zksync_types::InputData {
                    hash: Default::default(),
                    data: Default::default(),
                })
            }
        }

        let tx_results_pretty = TransactionSummary::new(l2_gas_price, &tx, &tx_result, None);
        let resolve_hashes = get_env_var::<bool>("ZK_DEBUG_RESOLVE_HASHES");

        sh_println!("{tx_results_pretty}").unwrap();

        if !call_traces.is_empty() {
            let call_traces = call_traces.clone();
            let tx_result_for_arena = tx_result.clone();
            let mut builder = CallTraceDecoderBuilder::default();
            builder = builder.with_signature_identifier(
                SignaturesIdentifier::new(None, !resolve_hashes)
                    .expect("failed building signature identifier"),
            );

            let decoder = builder.build();
            let arena = futures::executor::block_on(async {
                let blocking_result = tokio::task::spawn_blocking(move || {
                    let mut arena = build_call_trace_arena(&call_traces, &tx_result_for_arena);
                    let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
                    rt.block_on(async { decode_trace_arena(&mut arena, &decoder).await });
                    arena
                })
                .await;

                blocking_result.expect("spawn_blocking failed")
            });

            let filtered_arena = filter_call_trace_arena(&arena, 2);
            let trace_output = render_trace_arena_inner(&filtered_arena, false);
            sh_println!("\nTraces:\n{}", trace_output).expect("failed printing zkEVM traces");
        }
    }

    let bytecodes = compressed_bytecodes
        .into_iter()
        .map(|b| {
            zksync_types::bytecode::validate_bytecode(&b.original).expect("invalid bytecode");
            let hash = BytecodeHash::for_bytecode(&b.original).value();

            (hash, b.original)
        })
        .collect::<HashMap<_, _>>();
    let modified_storage = storage.borrow().modified_storage_keys().clone();

    // patch CREATE traces.
    for call in call_traces.iter_mut() {
        call_traces_patch_create(&deployed_bytecode_hashes, &bytecodes, storage.clone(), call);
    }

    // define a CREATE outcome that contains the additional data necessary for upstream to set up
    // the output result.
    let create_outcome = if is_create {
        match &tx_result.result {
            ExecutionResult::Success { output } => {
                let result = ethabi::decode(&[ethabi::ParamType::Bytes], output)
                    .ok()
                    .and_then(|result| result.first().cloned())
                    .and_then(|result| result.into_bytes())
                    .unwrap_or_default();

                if result.len() == 32 {
                    let address = h256_to_address(&H256::from_slice(&result));
                    deployed_bytecode_hashes.get(&address).cloned().and_then(|hash| {
                        bytecodes
                            .get(&hash)
                            .cloned()
                            .or_else(|| storage.borrow_mut().load_factory_dep(hash))
                            .map(|bytecode| InnerCreateOutcome { address, hash, bytecode })
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    } else {
        None
    };

    if is_static {
        InnerZkVmResult {
            tx_result,
            bytecodes: Default::default(),
            modified_storage: Default::default(),
            call_traces,
            create_outcome,
            gas_usage,
            recorded_immutables: Default::default(),
        }
    } else {
        InnerZkVmResult {
            tx_result,
            bytecodes,
            modified_storage,
            call_traces,
            create_outcome,
            gas_usage,
            recorded_immutables,
        }
    }
}

/// Patch CREATE traces with bytecode as the data is empty bytes.
fn call_traces_patch_create<S: ReadStorage>(
    deployed_bytecode_hashes: &HashMap<H160, H256>,
    bytecodes: &HashMap<H256, Vec<u8>>,
    storage: StoragePtr<StorageView<S>>,
    call: &mut Call,
) {
    if matches!(call.r#type, CallType::Create) {
        if let Some(hash) = deployed_bytecode_hashes.get(&call.to).cloned() {
            let maybe_bytecode = bytecodes
                .get(&hash)
                .cloned()
                .or_else(|| storage.borrow_mut().load_factory_dep(hash));
            if let Some(bytecode) = maybe_bytecode {
                call.output = bytecode;
            }
        }
    }
    for subcall in &mut call.calls {
        call_traces_patch_create(deployed_bytecode_hashes, bytecodes, storage.clone(), subcall);
    }
}
/// Parse solidity's `console.log` events
struct ConsoleLogParser {
    hardhat_console_address: H160,
}

impl ConsoleLogParser {
    fn new() -> Self {
        Self { hardhat_console_address: HARDHAT_CONSOLE_ADDRESS.to_h160() }
    }

    pub fn get_logs(&self, call_traces: &[Call], print: bool) -> Vec<Log> {
        let mut logs = vec![];
        for call in call_traces {
            self.parse_call_recursive(call, &mut logs, print);
        }
        logs
    }

    fn parse_call_recursive(&self, current_call: &Call, logs: &mut Vec<Log>, print: bool) {
        self.parse_call(current_call, logs, print);
        for call in &current_call.calls {
            self.parse_call_recursive(call, logs, print);
        }
    }

    fn parse_call(&self, current_call: &Call, logs: &mut Vec<Log>, print: bool) {
        use alloy_sol_types::{SolEvent, SolInterface, SolValue};
        use foundry_common::fmt::ConsoleFmt;

        if current_call.to != self.hardhat_console_address {
            return;
        }
        if current_call.input.len() < 4 {
            return;
        }

        let input = current_call.input.clone();

        let Ok(call) = console::hh::ConsoleCalls::abi_decode(&input) else {
            return;
        };

        // Convert the parameters of the call to their string representation using `ConsoleFmt`.
        let message = call.fmt(Default::default());
        let log = Log::new(
            Address::default(),
            vec![Console::log::SIGNATURE_HASH],
            message.abi_encode().into(),
        )
        .unwrap_or_else(|| Log { ..Default::default() });

        logs.push(log);

        if print {
            info!("{}", ansiterm::Color::Cyan.paint(message));
        }
    }
}

fn get_env_var<T>(name: &str) -> T
where
    T: std::str::FromStr + Default,
    T::Err: Debug,
{
    std::env::var(name)
        .map(|value| {
            value
                .parse::<T>()
                .unwrap_or_else(|err| panic!("failed parsing env variable {name}={value}, {err:?}"))
        })
        .unwrap_or_default()
}

/// Maximum size allowed for factory_deps during create.
/// We batch factory_deps till this upper limit if there are multiple deps.
/// These batches are then deployed individually.
static MAX_FACTORY_DEPENDENCIES_SIZE_BYTES: LazyLock<usize> = LazyLock::new(|| {
    std::env::var("MAX_FACTORY_DEPENDENCIES_SIZE_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(100_000)
});

/// Batch factory deps on the basis of size.
///
/// For large factory_deps the VM can run out of gas. To avoid this case we batch factory_deps
/// on the basis of the max factory dependencies size and deploy all but the last batch
/// via empty transactions, with the last one deployed normally via create.
pub fn batch_factory_dependencies(mut factory_deps: Vec<Vec<u8>>) -> Vec<Vec<Vec<u8>>> {
    let factory_deps_count = factory_deps.len();
    let factory_deps_sizes = factory_deps.iter().map(|dep| dep.len()).collect_vec();
    let factory_deps_total_size = factory_deps_sizes.iter().sum::<usize>();
    tracing::debug!(count=factory_deps_count, total=factory_deps_total_size, sizes=?factory_deps_sizes, max=*MAX_FACTORY_DEPENDENCIES_SIZE_BYTES, "optimizing factory_deps");

    let mut batches = vec![];
    let mut current_batch = vec![];
    let mut current_batch_len = 0;

    // sort in increasing order of size to ensure the smaller bytecodes are packed efficiently
    factory_deps.sort_by_key(|a| a.len());
    for dep in factory_deps {
        let len = dep.len();
        let new_len = current_batch_len + len;
        if new_len > *MAX_FACTORY_DEPENDENCIES_SIZE_BYTES && !current_batch.is_empty() {
            batches.push(current_batch);
            current_batch = vec![];
            current_batch_len = 0;
        }
        current_batch.push(dep);
        current_batch_len += len;
    }

    if !current_batch.is_empty() {
        batches.push(current_batch);
    }

    let batch_count = batches.len();
    let batch_individual_sizes =
        batches.iter().map(|deps| deps.iter().map(|dep| dep.len()).collect_vec()).collect_vec();
    let batch_cumulative_sizes =
        batches.iter().map(|deps| deps.iter().map(|dep| dep.len()).sum::<usize>()).collect_vec();
    let batch_total_size = batch_cumulative_sizes.iter().sum::<usize>();
    tracing::info!(count=batch_count, total=batch_total_size, sizes=?batch_cumulative_sizes, batched_sizes=?batch_individual_sizes, "optimized factory_deps into batches");

    batches
}

/// Transforms a given L2Tx into multiple txs if the factory deps need to be batched
fn split_tx_by_factory_deps(mut tx: L2Tx) -> Vec<L2Tx> {
    if tx.execute.factory_deps.is_empty() {
        return vec![tx];
    }

    let mut batched = batch_factory_dependencies(tx.execute.factory_deps);
    let last_deps = batched.pop().unwrap_or_default();

    let mut txs = Vec::with_capacity(batched.len() + 1);
    for deps in batched.into_iter() {
        txs.push(L2Tx::new(
            Some(H160::zero()),
            Vec::default(),
            tx.common_data.nonce,
            tx.common_data.fee.clone(),
            tx.common_data.initiator_address,
            Default::default(),
            deps,
            tx.common_data.paymaster_params.clone(),
        ));
        tx.common_data.nonce = Nonce(tx.common_data.nonce.0.saturating_add(1));
    }

    tx.execute.factory_deps = last_deps;
    txs.push(tx);

    txs
}
