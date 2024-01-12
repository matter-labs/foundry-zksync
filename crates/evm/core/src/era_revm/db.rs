/// RevmDatabaseForEra allows era VM to use the revm "Database" object
/// as a read-only fork source.
/// This way, we can run transaction on top of the chain that is persisted
/// in the Database object.
/// This code doesn't do any mutatios to Database: after each transaction run, the Revm
/// is usually collecing all the diffs - and applies them to database itself.
use std::{
    collections::HashMap,
    fmt::Debug,
    sync::{Arc, Mutex},
};

use foundry_common::zk_utils::conversion_utils::{
    h160_to_address, h256_to_b256, h256_to_h160, revm_u256_to_h256, u256_to_revm_u256,
};
use revm::{
    primitives::{Bytecode, Bytes},
    Database,
};
use zksync_basic_types::{web3::signing::keccak256, AccountTreeId, L2ChainId, H160, H256, U256};
use zksync_state::ReadStorage;
use zksync_types::{
    block::unpack_block_info, get_code_key, get_system_context_init_logs, StorageKey, StorageLog,
    StorageLogKind, ACCOUNT_CODE_STORAGE_ADDRESS, L2_ETH_TOKEN_ADDRESS, NONCE_HOLDER_ADDRESS,
    SYSTEM_CONTEXT_ADDRESS, SYSTEM_CONTEXT_BLOCK_INFO_POSITION,
    SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
};

use super::storage_view::StorageView;
use zksync_utils::{address_to_h256, bytecode::hash_bytecode, h256_to_u256, u256_to_h256};

#[derive(Default)]
pub struct RevmDatabaseForEra<DB> {
    pub db: Arc<Mutex<Box<DB>>>,
    pub current_block: u64,
    pub factory_deps: HashMap<H256, Vec<u8>>,
}

impl<Db> Clone for RevmDatabaseForEra<Db> {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            current_block: self.current_block,
            factory_deps: self.factory_deps.clone(),
        }
    }
}

impl<DB> Debug for RevmDatabaseForEra<DB> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RevmDatabaseForEra")
            .field("db", &"db")
            // .field("env", &self.env.lock().unwrap())
            .finish()
    }
}

impl<DB: Database + Send> RevmDatabaseForEra<DB>
where
    <DB as revm::Database>::Error: Debug,
{
    /// Create a new instance of [RevmDatabaseForEra].
    pub fn new(db: Arc<Mutex<Box<DB>>>) -> Self {
        let db_inner = db.clone();
        let current_block = {
            let mut db = db_inner.lock().expect("failed aquiring lock on the database");
            let result = db
                .storage(h160_to_address(SYSTEM_CONTEXT_ADDRESS), u256_to_revm_u256(U256::from(9)))
                .unwrap();
            let num_and_ts = revm_u256_to_h256(result);
            let (num, _) = unpack_block_info(h256_to_u256(num_and_ts));
            num
        };
        Self { db, current_block: current_block as u64, factory_deps: HashMap::new() }
    }

    pub fn into_storage_view_with_system_contracts(
        mut self,
        mut modified_keys: HashMap<StorageKey, H256>,
    ) -> StorageView<Self> {
        let contracts = era_test_node::system_contracts::get_deployed_contracts(
            &era_test_node::system_contracts::Options::BuiltInWithoutSecurity,
        );
        // TODO fix chain id
        let chain_id = { L2ChainId::try_from(9u32).unwrap() };
        let system_context_init_log = get_system_context_init_logs(chain_id);

        contracts
            .iter()
            .map(|contract| {
                let deployer_code_key = get_code_key(contract.account_id.address());
                StorageLog::new_write_log(deployer_code_key, hash_bytecode(&contract.bytecode))
            })
            .chain(system_context_init_log)
            .for_each(|log| {
                (log.kind == StorageLogKind::Write)
                    .then_some(modified_keys.insert(log.key, log.value));
            });

        let factory_deps = contracts
            .into_iter()
            .map(|contract| (hash_bytecode(&contract.bytecode), contract.bytecode))
            .collect();

        self.factory_deps = factory_deps;
        let mut storage_view = StorageView::new(self);
        storage_view.modified_storage_keys = modified_keys;
        storage_view
    }

    /// Returns the current L1 block number and timestamp from the database.
    /// Reads it directly from the SYSTEM_CONTEXT storage.
    pub fn get_l1_block_number_and_timestamp(&self) -> (u64, u64) {
        let num_and_ts = self.read_storage_internal(
            SYSTEM_CONTEXT_ADDRESS,
            h256_to_u256(SYSTEM_CONTEXT_BLOCK_INFO_POSITION),
        );
        let (num, ts) = unpack_block_info(h256_to_u256(num_and_ts));
        (num, ts)
    }

    /// Returns the current L2 block number and timestamp from the database.
    /// Reads it directly from the SYSTEM_CONTEXT storage.
    pub fn get_l2_block_number_and_timestamp(&self) -> (u64, u64) {
        let num_and_ts = self.read_storage_internal(
            SYSTEM_CONTEXT_ADDRESS,
            h256_to_u256(SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION),
        );
        let (num, ts) = unpack_block_info(h256_to_u256(num_and_ts));
        (num, ts)
    }

    /// Returns the nonce for a given account from NonceHolder storage.
    pub fn get_nonce_for_address(&self, address: H160) -> u64 {
        // Nonce is stored in the first mapping of the Nonce contract.
        let storage_idx = [&[0; 12], address.as_bytes(), &[0; 32]].concat();
        let storage_idx = H256::from_slice(&keccak256(storage_idx.as_slice()));

        let nonce_storage =
            self.read_storage_internal(NONCE_HOLDER_ADDRESS, h256_to_u256(storage_idx));
        let nonces: [u8; 8] = nonce_storage.as_fixed_bytes()[24..32].try_into().unwrap();
        u64::from_be_bytes(nonces)
    }

    fn read_storage_internal(&self, address: H160, idx: U256) -> H256 {
        let mut db = self.db.lock().unwrap();
        let result = db.storage(h160_to_address(address), u256_to_revm_u256(idx)).unwrap();
        revm_u256_to_h256(result)
    }

    /// Tries to fetch the bytecode that belongs to a given account.
    /// Start, by looking into account code storage - to see if there is any information about the
    /// bytecode for this account. If there is none - check if any of the bytecode hashes are
    /// matching the account. And as the final step - read the bytecode from the database
    /// itself.
    pub fn fetch_account_code(
        &self,
        account: H160,
        modified_keys: &HashMap<StorageKey, H256>,
        bytecodes: &HashMap<U256, Vec<U256>>,
    ) -> Option<(H256, Bytecode)> {
        // First - check if the bytecode was set/changed in the recent block.
        if let Some(v) = modified_keys.get(&StorageKey::new(
            AccountTreeId::new(ACCOUNT_CODE_STORAGE_ADDRESS),
            address_to_h256(&account),
        )) {
            let new_bytecode_hash = *v;
            if let Some(new_bytecode) = bytecodes.get(&h256_to_u256(new_bytecode_hash)) {
                let u8_bytecode: Vec<u8> =
                    new_bytecode.iter().flat_map(|x| u256_to_h256(*x).to_fixed_bytes()).collect();

                return Some((
                    new_bytecode_hash,
                    Bytecode {
                        bytecode: Bytes::copy_from_slice(u8_bytecode.as_slice()),
                        state: revm::primitives::BytecodeState::Raw,
                    },
                ))
            }
        }

        // Check if maybe we got a bytecode with this hash.
        // Unfortunately the accounts are mapped as "last 20 bytes of the 32 byte hash".
        // so we have to iterate over all the bytecodes, truncate their hash and then compare.
        for (k, v) in bytecodes {
            if h256_to_h160(&u256_to_h256(*k)) == account {
                let u8_bytecode: Vec<u8> =
                    v.iter().flat_map(|x| u256_to_h256(*x).to_fixed_bytes()).collect();

                return Some((
                    u256_to_h256(*k),
                    Bytecode {
                        bytecode: Bytes::copy_from_slice(u8_bytecode.as_slice()),
                        state: revm::primitives::BytecodeState::Raw,
                    },
                ))
            }
        }

        let account = h160_to_address(account);

        let mut db = self.db.lock().unwrap();
        db.basic(account)
            .ok()
            .and_then(|db_account| {
                db_account.map(|a| a.code.map(|b| (H256::from(a.code_hash.0), b)))
            })
            .flatten()
    }

    pub fn store_factory_dep(&mut self, hash: H256, bytecode: Vec<u8>) {
        self.factory_deps.insert(hash, bytecode);
    }
}

impl<DB> ReadStorage for RevmDatabaseForEra<DB>
where
    DB: Database + Send,
    <DB as revm::Database>::Error: Debug,
{
    fn read_value(&mut self, key: &StorageKey) -> zksync_types::StorageValue {
        let mut result = self.read_storage_internal(*key.address(), h256_to_u256(*key.key()));
        if L2_ETH_TOKEN_ADDRESS == *key.address() && result.is_zero() {
            // TODO: here we should read the account information from the Database trait
            // and lookup how many token it holds.
            // Unfortunately the 'idx' here is a hash of the account and Database doesn't
            // support getting a list of active accounts.
            // So for now - simply assume that every user has infinite money.
            result = u256_to_h256(U256::from(9_223_372_036_854_775_808_u64));
        }
        result
    }

    fn is_write_initial(&mut self, _key: &StorageKey) -> bool {
        false
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        let mut db = self.db.lock().unwrap();
        let result = db.code_by_hash(h256_to_b256(hash));
        let res = match result {
            Ok(bytecode) => {
                if bytecode.is_empty() {
                    return self.factory_deps.get(&hash).cloned()
                }
                Some(bytecode.bytecode.to_vec())
            }
            Err(_) => self.factory_deps.get(&hash).cloned(),
        };
        res
    }

    fn get_enumeration_index(&mut self, _key: &StorageKey) -> Option<u64> {
        Some(0_u64)
    }
}
