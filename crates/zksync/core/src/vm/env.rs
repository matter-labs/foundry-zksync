use zksync_basic_types::{AccountTreeId, L1BatchNumber, L2BlockNumber, L2ChainId, H160};
use zksync_contracts::BaseSystemContracts;
use zksync_multivm::{
    interface::{L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode},
    vm_latest::{constants::BATCH_COMPUTATIONAL_GAS_LIMIT, utils::l2_blocks::load_last_l2_block},
};
use zksync_types::{
    block::{unpack_block_info, L2BlockHasher},
    fee_model::PubdataIndependentBatchFeeModelInput,
    h256_to_u256, StorageKey, SYSTEM_CONTEXT_ADDRESS, SYSTEM_CONTEXT_BLOCK_INFO_POSITION,
};
use zksync_vm_interface::storage::{ReadStorage, StoragePtr};

// https://github.com/matter-labs/era-contracts/blob/aafee035db892689df3f7afe4b89fd6467a39313/system-contracts/bootloader/bootloader.yul#L86
const MAX_L2_GAS_PER_PUBDATA: u64 = 50000;

#[derive(Debug, Clone)]
/// Values related to the era vm environment
pub struct ZkEnv {
    /// l1 gas price
    pub l1_gas_price: u64,
    /// fair l2 gas price
    pub fair_l2_gas_price: u64,
    /// fair pubdata price
    pub fair_pubdata_price: u64,
}

impl Default for ZkEnv {
    fn default() -> Self {
        // TODO: fair pubdata price of 0 yields division by 0 error somewhere in
        // some cases. Should investigate this edge case further
        Self { l1_gas_price: 0, fair_l2_gas_price: 0, fair_pubdata_price: 1000 }
    }
}

impl ZkEnv {
    /// Compute gas per pubdata
    pub fn gas_per_pubdata(&self) -> u64 {
        // source: https://github.com/matter-labs/era-contracts/blob/aafee035db892689df3f7afe4b89fd6467a39313/system-contracts/bootloader/bootloader.yul#L59
        let base_fee = std::cmp::max(
            self.fair_l2_gas_price,
            self.fair_pubdata_price.div_ceil(MAX_L2_GAS_PER_PUBDATA),
        );
        if base_fee == 0 {
            0
        } else {
            self.fair_pubdata_price.div_ceil(base_fee)
        }
    }
}

pub(crate) fn create_l1_batch_env<ST: ReadStorage>(
    storage: StoragePtr<ST>,
    zk_env: &ZkEnv,
) -> L1BatchEnv {
    let mut first_l2_block = if let Some(last_l2_block) = load_last_l2_block(&storage) {
        L2BlockEnv {
            number: last_l2_block.number + 1,
            timestamp: last_l2_block.timestamp + 1,
            prev_block_hash: last_l2_block.hash,
            max_virtual_blocks_to_create: 1,
        }
    } else {
        // This is the scenario of either the first L2 block ever
        L2BlockEnv {
            number: 1,
            timestamp: 1,
            prev_block_hash: L2BlockHasher::legacy_hash(L2BlockNumber(0)),
            max_virtual_blocks_to_create: 1,
        }
    };
    let (mut batch_number, mut batch_timestamp) = load_last_l1_batch(storage).unwrap_or_default();

    batch_number += 1;

    first_l2_block.timestamp = std::cmp::max(batch_timestamp + 1, first_l2_block.timestamp);
    batch_timestamp = first_l2_block.timestamp;
    tracing::info!(
        zk_env.fair_l2_gas_price,
        zk_env.l1_gas_price,
        zk_env.fair_pubdata_price,
        "batch env"
    );
    L1BatchEnv {
        // TODO: set the previous batch hash properly (take from fork, when forking, and from local
        // storage, when this is not the first block).
        previous_batch_hash: None,
        number: L1BatchNumber::from(batch_number as u32),
        timestamp: batch_timestamp,

        fee_account: H160::zero(),
        enforced_base_fee: None,
        first_l2_block,
        fee_input: zksync_types::fee_model::BatchFeeInput::PubdataIndependent(
            PubdataIndependentBatchFeeModelInput {
                fair_l2_gas_price: zk_env.fair_l2_gas_price,
                l1_gas_price: zk_env.l1_gas_price,
                fair_pubdata_price: zk_env.fair_pubdata_price,
            },
        ),
    }
}

pub(crate) fn create_system_env(
    base_system_contracts: BaseSystemContracts,
    chain_id: L2ChainId,
) -> SystemEnv {
    SystemEnv {
        zk_porter_available: false,
        // TODO: when forking, we could consider taking the protocol version id from the fork
        // itself.
        version: zksync_types::ProtocolVersionId::latest(),
        base_system_smart_contracts: base_system_contracts,
        bootloader_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
        execution_mode: TxExecutionMode::VerifyExecute,
        default_validation_computational_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
        chain_id,
    }
}

pub(crate) fn load_last_l1_batch<S: ReadStorage>(storage: StoragePtr<S>) -> Option<(u64, u64)> {
    // Get block number and timestamp
    let current_l1_batch_info_key = StorageKey::new(
        AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
        SYSTEM_CONTEXT_BLOCK_INFO_POSITION,
    );
    let mut storage_ptr = storage.borrow_mut();
    let current_l1_batch_info = storage_ptr.read_value(&current_l1_batch_info_key);
    let (batch_number, batch_timestamp) = unpack_block_info(h256_to_u256(current_l1_batch_info));
    let block_number = batch_number as u32;
    if block_number == 0 {
        // The block does not exist yet
        return None;
    }
    Some((batch_number, batch_timestamp))
}
