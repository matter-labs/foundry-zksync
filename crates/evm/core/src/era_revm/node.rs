use std::{collections::HashMap, sync::Arc};

use era_test_node::{
    console_log::ConsoleLogHandler,
    formatter,
    node::ShowCalls,
    system_contracts::{Options, SystemContracts},
    utils::bytecode_to_factory_dep,
};
use multivm::{
    interface::{VmExecutionResultAndLogs, VmInterface},
    tracers::CallTracer,
    vm_latest::{HistoryDisabled, ToTracerPointer, TracerPointer, Vm, VmExecutionMode},
};
use once_cell::sync::OnceCell;
use zksync_basic_types::{L2ChainId, H256};
use zksync_state::{ReadStorage, StoragePtr, WriteStorage};
use zksync_types::{l2::L2Tx, StorageKey, Transaction, U256};

use crate::era_revm::env::{create_l1_batch_env, create_system_env};

use super::storage_view::StorageView;

/// Executes the given L2 transaction and returns all the VM logs.
pub fn run_l2_tx_raw<S: ReadStorage>(
    l2_tx: L2Tx,
    storage: StoragePtr<StorageView<S>>,
    chain_id: L2ChainId,
    l1_gas_price: u64,
    mut tracers: Vec<TracerPointer<StorageView<S>, multivm::vm_latest::HistoryDisabled>>,
) -> (VmExecutionResultAndLogs, HashMap<U256, Vec<U256>>, HashMap<StorageKey, H256>) {
    let batch_env = create_l1_batch_env(storage.clone(), l1_gas_price);

    let system_contracts = SystemContracts::from_options(&Options::BuiltInWithoutSecurity);
    let system_env = create_system_env(system_contracts.baseline_contracts, chain_id);

    let mut vm: Vm<_, HistoryDisabled> = Vm::new(batch_env.clone(), system_env, storage.clone());

    let tx: Transaction = l2_tx.clone().into();

    vm.push_transaction(tx.clone());
    let call_tracer_result = Arc::new(OnceCell::default());
    tracers.push(CallTracer::new(call_tracer_result.clone()).into_tracer_pointer());

    let tx_result = vm.inspect(tracers.into(), VmExecutionMode::OneTx);
    let call_traces = Arc::try_unwrap(call_tracer_result).unwrap().take().unwrap_or_default();

    tracing::info!("=== Console Logs: ");
    let console_log_handler = ConsoleLogHandler::default();
    for call in &call_traces {
        console_log_handler.handle_call_recursive(call);
    }

    tracing::info!("=== Calls: ");
    for call in call_traces.iter() {
        formatter::print_call(call, 0, &ShowCalls::None, false);
    }

    let bytecodes: HashMap<U256, Vec<U256>> = vm
        .get_last_tx_compressed_bytecodes()
        .iter()
        .map(|b| bytecode_to_factory_dep(b.original.clone()))
        .collect();
    let modified_keys = storage.borrow().modified_storage_keys().clone();
    (tx_result, bytecodes, modified_keys)
}
