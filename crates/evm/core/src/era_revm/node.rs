use std::collections::HashMap;

use era_test_node::{
    system_contracts::{Options, SystemContracts},
    utils::bytecode_to_factory_dep,
};
use multivm::{
    interface::{VmExecutionResultAndLogs, VmInterface},
    vm_latest::{HistoryDisabled, TracerPointer, Vm, VmExecutionMode},
};
use zksync_basic_types::{L2ChainId, H256};
use zksync_state::{ReadStorage, StoragePtr, WriteStorage};
use zksync_types::{l2::L2Tx, StorageKey, Transaction, U256};

use crate::era_revm::env::{create_l1_batch_env, create_system_env};

use super::storage_view::StorageView;

/// Executes the given L2 transaction and returns all the VM logs.
///
/// **NOTE**
///
/// This function must only rely on data populated initially via [ForkDetails]:
///     * [InMemoryNodeInner::current_timestamp]
///     * [InMemoryNodeInner::current_batch]
///     * [InMemoryNodeInner::current_miniblock]
///     * [InMemoryNodeInner::current_miniblock_hash]
///     * [InMemoryNodeInner::l1_gas_price]
///
/// And must _NEVER_ rely on data updated in [InMemoryNodeInner] during previous runs:
/// (if used, they must never panic and/or have meaningful defaults)
///     * [InMemoryNodeInner::block_hashes]
///     * [InMemoryNodeInner::blocks]
///     * [InMemoryNodeInner::tx_results]
///
/// This is because external users of the library may call this function to perform an isolated
/// VM operation with an external storage and get the results back.
/// So any data populated in [Self::run_l2_tx] will not be available for the next invocation.
pub fn run_l2_tx_raw<S: ReadStorage>(
    l2_tx: L2Tx,
    storage: StoragePtr<StorageView<S>>,
    chain_id: L2ChainId,
    l1_gas_price: u64,
    tracers: Vec<TracerPointer<StorageView<S>, multivm::vm_latest::HistoryDisabled>>,
) -> (VmExecutionResultAndLogs, HashMap<U256, Vec<U256>>, HashMap<StorageKey, H256>) {
    let batch_env = create_l1_batch_env(storage.clone(), l1_gas_price);

    let system_contracts = SystemContracts::from_options(&Options::BuiltInWithoutSecurity);
    let system_env = create_system_env(system_contracts.baseline_contracts, chain_id);

    dbg!(&batch_env, &system_env);
    let mut vm: Vm<_, HistoryDisabled> = Vm::new(batch_env.clone(), system_env, storage.clone());

    let tx: Transaction = l2_tx.clone().into();

    vm.push_transaction(tx.clone());

    let tx_result = vm.inspect(tracers.into(), VmExecutionMode::OneTx);

    // tracing::info!("┌─────────────────────────┐");
    // tracing::info!("│   TRANSACTION SUMMARY   │");
    // tracing::info!("└─────────────────────────┘");

    // tracing::info!("");
    // tracing::info!("==== Console logs: ");
    // TODO print console logs

    let bytecodes: HashMap<U256, Vec<U256>> = vm
        .get_last_tx_compressed_bytecodes()
        .iter()
        .map(|b| bytecode_to_factory_dep(b.original.clone()))
        .collect();
    let modified_keys = storage.borrow().modified_storage_keys().clone();
    (tx_result, bytecodes, modified_keys)
}
