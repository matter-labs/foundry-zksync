use itertools::Itertools;
use zksync_basic_types::{Nonce, H160};
use zksync_types::l2::L2Tx;

/// Maximum size allowed for factory_deps during create.
/// We batch factory_deps till this upper limit if there are multiple deps.
/// These batches are then deployed individually
pub const MAX_FACTORY_DEPENDENCIES_SIZE_BYTES: usize = 100000; // 100kB

/// Batch factory deps on the basis of size.
///
/// For large factory_deps the VM can run out of gas. To avoid this case we batch factory_deps
/// on the basis of [MAX_FACTORY_DEPENDENCIES_SIZE_BYTES] and deploy all but the last batch
/// via empty transactions, with the last one deployed normally via create.
fn batch_factory_dependencies(mut factory_deps: Vec<Vec<u8>>) -> Vec<Vec<Vec<u8>>> {
    let factory_deps_count = factory_deps.len();
    let factory_deps_sizes = factory_deps.iter().map(|dep| dep.len()).collect_vec();
    let factory_deps_total_size = factory_deps_sizes.iter().sum::<usize>();
    tracing::debug!(count=factory_deps_count, total=factory_deps_total_size, sizes=?factory_deps_sizes, "optimizing factory_deps");

    let mut batches = vec![];
    let mut current_batch = vec![];
    let mut current_batch_len = 0;

    // sort in increasing order of size to ensure the smaller bytecodes are packed efficiently
    factory_deps.sort_by_key(|a| a.len());
    for dep in factory_deps {
        let len = dep.len();
        let new_len = current_batch_len + len;
        if new_len > MAX_FACTORY_DEPENDENCIES_SIZE_BYTES && !current_batch.is_empty() {
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
    tracing::debug!(count=batch_count, total=batch_total_size, sizes=?batch_cumulative_sizes, batched_sizes=?batch_individual_sizes, "optimized factory_deps into batches");

    batches
}

/// Transforms a given L2Tx into multiple txs if the factory deps need to be batched
pub fn split_tx_by_factory_deps(mut tx: L2Tx) -> Vec<L2Tx> {
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
