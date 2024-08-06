use alloy_primitives::Log;
use era_test_node::{
    formatter,
    node::ShowCalls,
    system_contracts::{Options, SystemContracts},
    utils::bytecode_to_factory_dep,
};
use foundry_common::{Console, HardhatConsole, HARDHAT_CONSOLE_ADDRESS};
use itertools::Itertools;
use multivm::{
    interface::{Halt, VmInterface, VmRevertReason},
    tracers::CallTracer,
    vm_latest::{
        ExecutionResult, HistoryDisabled, ToTracerPointer, Vm, VmExecutionMode,
        VmExecutionResultAndLogs,
    },
};
use once_cell::sync::OnceCell;
use revm::{
    db::states::StorageSlot,
    primitives::{
        Address, Bytecode, Bytes, EVMResultGeneric, ExecutionResult as rExecutionResult,
        HaltReason, HashMap as rHashMap, Log as rLog, OutOfGasError, Output, SuccessReason, B256,
        U256 as rU256,
    },
    Database, EvmContext,
};
use tracing::{debug, error, info, trace, warn};
use zksync_basic_types::{ethabi, L2ChainId, Nonce, H160, H256, U256};
use zksync_state::{ReadStorage, StoragePtr, WriteStorage};
use zksync_types::{
    l2::L2Tx, vm_trace::Call, PackedEthSignature, StorageKey, Transaction, VmEvent,
    ACCOUNT_CODE_STORAGE_ADDRESS,
};
use zksync_utils::{h256_to_account_address, h256_to_u256, u256_to_h256};

use std::{collections::HashMap, fmt::Debug, sync::Arc};

use crate::{
    convert::{ConvertAddress, ConvertH160, ConvertH256, ConvertU256},
    is_system_address,
    vm::{
        db::{ZKVMData, DEFAULT_CHAIN_ID},
        env::{create_l1_batch_env, create_system_env},
        storage_view::StorageView,
        tracer::{CallContext, CheatcodeTracer, CheatcodeTracerContext},
    },
};

/// Maximum gas price allowed for L1.
const MAX_L1_GAS_PRICE: u64 = 1000;

/// Represents the result of execution a [`L2Tx`] on EraVM
#[derive(Debug)]
pub struct ZKVMExecutionResult {
    /// The logs of a given execution
    pub logs: Vec<rLog>,
    /// The result of a given execution
    pub execution_result: rExecutionResult,
}

/// Revm-style result with ZKVM Execution
pub type ZKVMResult<E> = EVMResultGeneric<ZKVMExecutionResult, E>;

/// Same as [`inspect`] but batches factory deps to account for size limitations.
///
/// Will handle aggregating execution results, where errors, reverts or halts will be propagated
/// immediately.
/// All logs will be collected as they happen, and returned with the final result.
//TODO: should we make this transparent in `inspect` directly?
pub fn inspect_as_batch<DB, E>(
    tx: L2Tx,
    ecx: &mut EvmContext<DB>,
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
        let gas_used = aggregated_result
            .as_ref()
            .map(|r| r.execution_result.gas_used())
            .map(U256::from)
            .unwrap_or_default();

        //deducted gas used so far
        tx.common_data.fee.gas_limit -= gas_used;

        info!("executing batched tx ({}/{})", idx + 1, total_txns);
        let mut result = inspect(tx, ecx, ccx, call_ctx.clone())?;

        match (&mut aggregated_result, result.execution_result) {
            (_, exec @ rExecutionResult::Revert { .. } | exec @ rExecutionResult::Halt { .. }) => {
                return Ok(ZKVMExecutionResult { logs: result.logs, execution_result: exec });
            }
            (None, exec) => {
                aggregated_result
                    .replace(ZKVMExecutionResult { logs: result.logs, execution_result: exec });
            }
            (
                Some(ZKVMExecutionResult {
                    logs: aggregated_logs,
                    execution_result:
                        rExecutionResult::Success {
                            reason: agg_reason,
                            gas_used: agg_gas_used,
                            gas_refunded: agg_gas_refunded,
                            logs: agg_logs,
                            output: agg_output,
                        },
                }),
                rExecutionResult::Success { reason, gas_used, gas_refunded, logs, output },
            ) => {
                aggregated_logs.append(&mut result.logs);
                *agg_reason = reason;
                *agg_gas_used += gas_used;
                *agg_gas_refunded += gas_refunded;
                agg_logs.extend(logs);
                *agg_output = output;
            }
            _ => unreachable!("aggregated result must only contain success"),
        }
    }

    Ok(aggregated_result.expect("must have result"))
}

/// Processes a [`L2Tx`] with EraVM and returns the final execution result and logs.
///
/// State changes will be reflected in the given `Env`, `DB`, `JournaledState`.
pub fn inspect<DB, E>(
    mut tx: L2Tx,
    ecx: &mut EvmContext<DB>,
    ccx: &mut CheatcodeTracerContext,
    call_ctx: CallContext,
) -> ZKVMResult<E>
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    let chain_id = if ecx.env.cfg.chain_id <= u32::MAX as u64 {
        L2ChainId::from(ecx.env.cfg.chain_id as u32)
    } else {
        warn!(provided = ?ecx.env.cfg.chain_id, using = DEFAULT_CHAIN_ID, "using default chain id as provided chain_id does not fit into u32");
        L2ChainId::from(DEFAULT_CHAIN_ID)
    };

    let persisted_factory_deps = ccx
        .persisted_factory_deps
        .as_ref()
        .map(|factory_deps| (*factory_deps).clone())
        .unwrap_or_default();

    let env_tx_gas_limit = ecx.env.tx.gas_limit;
    let mut era_db = ZKVMData::new_with_system_contracts(ecx, chain_id)
        .with_extra_factory_deps(persisted_factory_deps)
        .with_storage_accesses(ccx.accesses.take());

    let is_create = call_ctx.is_create;
    info!(?call_ctx, "executing transaction in zk vm");

    if tx.common_data.signature.is_empty() {
        // FIXME: This is a hack to make sure that the signature is not empty.
        // Fails without a signature here: https://github.com/matter-labs/zksync-era/blob/73a1e8ff564025d06e02c2689da238ae47bb10c3/core/lib/types/src/transaction_request.rs#L381
        tx.common_data.signature = PackedEthSignature::default().serialize_packed().into();
    }

    let modified_storage_keys = era_db.override_keys.clone();
    let storage_ptr =
        StorageView::new(&mut era_db, modified_storage_keys, tx.common_data.initiator_address)
            .into_rc_ptr();
    let (tx_result, bytecodes, modified_storage) =
        inspect_inner(tx, storage_ptr, chain_id, ccx, call_ctx);

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
                Some(h256_to_account_address(&H256::from_slice(&result)))
            } else {
                None
            };
            let output = if is_create {
                Output::Create(Bytes::from(result), address.map(ConvertH160::to_address))
            } else {
                Output::Call(Bytes::from(result))
            };

            ZKVMExecutionResult {
                logs: logs.clone(),
                execution_result: rExecutionResult::Success {
                    reason: SuccessReason::Return,
                    gas_used: tx_result.statistics.gas_used,
                    gas_refunded: tx_result.refunds.gas_refunded,
                    logs,
                    output,
                },
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
                execution_result: rExecutionResult::Revert {
                    gas_used: env_tx_gas_limit - tx_result.refunds.gas_refunded,
                    output: Bytes::from(output),
                },
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
                execution_result: rExecutionResult::Halt {
                    reason: mapped_reason,
                    gas_used: env_tx_gas_limit - tx_result.refunds.gas_refunded,
                },
            }
        }
    };

    // Insert into persisted_bytecodes. This is currently used in
    // deploying multiple factory_dep transactions in create to ensure
    // create does not OOG (Out of Gas) due to large factory deps.
    if let Some(persisted_factory_deps) = ccx.persisted_factory_deps.as_mut() {
        for (hash, bytecode) in &bytecodes {
            let bytecode =
                bytecode.iter().flat_map(|x| u256_to_h256(*x).to_fixed_bytes()).collect_vec();
            persisted_factory_deps.insert(hash.to_h256(), bytecode);
        }
    }

    let mut storage: rHashMap<Address, rHashMap<rU256, StorageSlot>> = Default::default();
    let mut codes: rHashMap<Address, (B256, Bytecode)> = Default::default();
    for (k, v) in &modified_storage {
        let address = k.address().to_address();
        let index = k.key().to_ru256();
        era_db.load_account(address);
        let previous = era_db.sload(address, index);
        let entry = storage.entry(address).or_default();
        entry.insert(index, StorageSlot::new_changed(previous, v.to_ru256()));

        if k.address() == &ACCOUNT_CODE_STORAGE_ADDRESS {
            if let Some(bytecode) = bytecodes.get(&h256_to_u256(*v)) {
                let bytecode =
                    bytecode.iter().flat_map(|x| u256_to_h256(*x).to_fixed_bytes()).collect_vec();
                let bytecode = Bytecode::new_raw(Bytes::from(bytecode));
                let hash = B256::from_slice(v.as_bytes());
                codes.insert(k.key().to_h160().to_address(), (hash, bytecode));
            } else {
                // We populate bytecodes for all non-system addresses
                if !is_system_address(k.key().to_h160().to_address()) {
                    if let Some(bytecode) = (&mut era_db).load_factory_dep(*v) {
                        let hash = B256::from_slice(v.as_bytes());
                        let bytecode = Bytecode::new_raw(Bytes::from(bytecode));
                        codes.insert(k.key().to_h160().to_address(), (hash, bytecode));
                    } else {
                        warn!(
                            "no bytecode was found for {:?}, requested by account {:?}",
                            *v,
                            k.key().to_h160()
                        );
                    }
                }
            }
        }
    }

    for (address, storage) in storage {
        ecx.load_account(address).expect("account could not be loaded");
        ecx.touch(&address);

        for (key, value) in storage {
            ecx.sstore(address, key, value.present_value).expect("failed writing to slot");
        }
    }

    for (address, (code_hash, code)) in codes {
        ecx.load_account(address).expect("account could not be loaded");
        ecx.touch(&address);
        let account = ecx.journaled_state.state.get_mut(&address).expect("account is loaded");

        account.info.code_hash = code_hash;
        account.info.code = Some(code);
    }

    Ok(execution_result)
}

fn inspect_inner<S: ReadStorage>(
    l2_tx: L2Tx,
    storage: StoragePtr<StorageView<S>>,
    chain_id: L2ChainId,
    ccx: &mut CheatcodeTracerContext,
    call_ctx: CallContext,
) -> (VmExecutionResultAndLogs, HashMap<U256, Vec<U256>>, HashMap<StorageKey, H256>) {
    let l1_gas_price = call_ctx.block_basefee.to::<u64>().max(MAX_L1_GAS_PRICE);
    let fair_l2_gas_price = call_ctx.block_basefee.saturating_to::<u64>();
    let batch_env = create_l1_batch_env(storage.clone(), l1_gas_price, fair_l2_gas_price);

    let system_contracts = SystemContracts::from_options(&Options::BuiltInWithoutSecurity);
    let system_env = create_system_env(system_contracts.baseline_contracts, chain_id);

    let mut vm: Vm<_, HistoryDisabled> = Vm::new(batch_env.clone(), system_env, storage.clone());

    let tx: Transaction = l2_tx.clone().into();

    vm.push_transaction(tx.clone());
    let call_tracer_result = Arc::new(OnceCell::default());
    let cheatcode_tracer_result = Arc::new(OnceCell::default());
    let mut expected_calls = HashMap::<_, _>::new();
    if let Some(ec) = &ccx.expected_calls {
        for (addr, v) in ec.iter() {
            expected_calls.insert(*addr, v.clone());
        }
    }
    let is_static = call_ctx.is_static;
    let tracers = vec![
        CallTracer::new(call_tracer_result.clone()).into_tracer_pointer(),
        CheatcodeTracer::new(
            ccx.mocked_calls.clone(),
            expected_calls,
            cheatcode_tracer_result.clone(),
            call_ctx,
        )
        .into_tracer_pointer(),
    ];
    let mut tx_result = vm.inspect(tracers.into(), VmExecutionMode::OneTx);
    let call_traces = Arc::try_unwrap(call_tracer_result).unwrap().take().unwrap_or_default();
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

    formatter::print_vm_details(&tx_result);

    info!("=== Console Logs: ");
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

    let resolve_hashes = get_env_var::<bool>("ZK_DEBUG_RESOLVE_HASHES");
    let show_outputs = get_env_var::<bool>("ZK_DEBUG_SHOW_OUTPUTS");
    info!("=== Calls: ");
    for call in call_traces.iter() {
        formatter::print_call(call, 0, &ShowCalls::All, show_outputs, resolve_hashes);
    }

    info!("==== {}", format!("{} events", tx_result.logs.events.len()));
    for event in &tx_result.logs.events {
        formatter::print_event(event, resolve_hashes);
    }

    let bytecodes = vm
        .get_last_tx_compressed_bytecodes()
        .iter()
        .map(|b| {
            bytecode_to_factory_dep(b.original.clone())
                .expect("failed converting bytecode to factory dep")
        })
        .collect();
    let modified_keys = if is_static {
        Default::default()
    } else {
        storage.borrow().modified_storage_keys().clone()
    };
    (tx_result, bytecodes, modified_keys)
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

        let mut input = current_call.input.clone();

        // Patch the Hardhat-style selector (`uint` instead of `uint256`)
        foundry_common::patch_hh_console_selector(&mut input);

        // Decode the call
        let Ok(call) = HardhatConsole::HardhatConsoleCalls::abi_decode(&input, false) else {
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
            info!("{}", ansi_term::Color::Cyan.paint(message));
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
            value.parse::<T>().unwrap_or_else(|err| {
                panic!("failed parsing env variable {}={}, {:?}", name, value, err)
            })
        })
        .unwrap_or_default()
}

lazy_static::lazy_static! {
    /// Maximum size allowed for factory_deps during create.
    /// We batch factory_deps till this upper limit if there are multiple deps.
    /// These batches are then deployed individually.
    ///
    /// TODO: This feature is disabled by default via `usize::MAX` due to inconsistencies
    /// with determining a value that works in all cases.
    static ref MAX_FACTORY_DEPENDENCIES_SIZE_BYTES: usize = std::env::var("MAX_FACTORY_DEPENDENCIES_SIZE_BYTES")
                                                            .ok()
                                                            .and_then(|value| value.parse::<usize>().ok())
                                                            .unwrap_or(usize::MAX);
}

/// Batch factory deps on the basis of size.
///
/// For large factory_deps the VM can run out of gas. To avoid this case we batch factory_deps
/// on the basis of [MAX_FACTORY_DEPENDENCIES_SIZE_BYTES] and deploy all but the last batch
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
    let Some(factory_deps) = tx.execute.factory_deps.take() else { return vec![tx] };

    let mut batched = batch_factory_dependencies(factory_deps);
    let last_deps = batched.pop();

    let mut txs = Vec::with_capacity(batched.len() + 1);
    for deps in batched.into_iter() {
        txs.push(L2Tx::new(
            H160::zero(),
            Vec::default(),
            tx.nonce(),
            tx.common_data.fee.clone(),
            tx.common_data.initiator_address,
            Default::default(),
            Some(deps),
            tx.common_data.paymaster_params.clone(),
        ));
        tx.common_data.nonce = Nonce(tx.nonce().0.saturating_add(1));
    }

    tx.execute.factory_deps = last_deps;
    txs.push(tx);

    txs
}
