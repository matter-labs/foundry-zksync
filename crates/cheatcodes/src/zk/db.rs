/// RevmDatabaseForEra allows era VM to use the revm "Database" object
/// as a read-only fork source.
/// This way, we can run transaction on top of the chain that is persisted
/// in the Database object.
/// This code doesn't do any mutatios to Database: after each transaction run, the Revm
/// is usually collecing all the diffs - and applies them to database itself.
use std::{collections::HashMap, fmt::Debug};

use alloy_primitives::Address;
use foundry_common::{
    conversion_utils::address_to_h160,
    zk_utils::conversion_utils::{
        h160_to_address, h256_to_b256, revm_u256_to_h256, u256_to_revm_u256,
    },
};
use revm::{Database, JournaledState};
use zksync_basic_types::{L2ChainId, H160, H256, U256};
use zksync_state::ReadStorage;
use zksync_types::{
    get_code_key, get_nonce_key, get_system_context_init_logs, utils::decompose_full_nonce, Nonce,
    StorageKey, StorageLog, StorageLogKind, StorageValue,
};

use zksync_utils::{bytecode::hash_bytecode, h256_to_u256};

pub struct ZKVMData<'a, DB> {
    pub db: &'a mut DB,
    pub journaled_state: &'a mut JournaledState,
    pub factory_deps: HashMap<H256, Vec<u8>>,
    pub override_keys: HashMap<StorageKey, StorageValue>,
}

impl<'a, DB> Debug for ZKVMData<'a, DB> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZKVMData")
            .field("db", &"db")
            .field("journaled_state", &"jouranled_state")
            .field("factory_deps", &self.factory_deps)
            .field("override_keys", &self.override_keys)
            .finish()
    }
}

impl<'a, DB> ZKVMData<'a, DB>
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    /// Create a new instance of [ZKEVMData].
    pub fn new(db: &'a mut DB, journaled_state: &'a mut JournaledState) -> Self {
        let factory_deps = journaled_state
            .state
            .values()
            .flat_map(|account| {
                if account.info.is_empty_code_hash() {
                    None
                } else if let Some(code) = &account.info.code {
                    Some((H256::from(account.info.code_hash.0), code.bytecode.to_vec()))
                } else {
                    None
                }
            })
            .collect::<HashMap<_, _>>();

        Self { db, journaled_state, factory_deps, override_keys: Default::default() }
    }

    /// Create a new instance of [ZKEVMData] with system contracts.
    pub fn new_with_system_contracts(
        db: &'a mut DB,
        journaled_state: &'a mut JournaledState,
    ) -> Self {
        let contracts = era_test_node::system_contracts::get_deployed_contracts(
            &era_test_node::system_contracts::Options::BuiltInWithoutSecurity,
        );
        let chain_id = { L2ChainId::try_from(31337u32).unwrap() };
        let system_context_init_log = get_system_context_init_logs(chain_id);

        let mut override_keys = HashMap::default();
        contracts
            .iter()
            .map(|contract| {
                let deployer_code_key = get_code_key(contract.account_id.address());
                StorageLog::new_write_log(deployer_code_key, hash_bytecode(&contract.bytecode))
            })
            .chain(system_context_init_log)
            .for_each(|log| {
                (log.kind == StorageLogKind::Write)
                    .then_some(override_keys.insert(log.key, log.value));
            });

        let mut factory_deps = contracts
            .into_iter()
            .map(|contract| (hash_bytecode(&contract.bytecode), contract.bytecode))
            .collect::<HashMap<_, _>>();
        factory_deps.extend(journaled_state.state.values().flat_map(|account| {
            if account.info.is_empty_code_hash() {
                None
            } else if let Some(code) = &account.info.code {
                Some((H256::from(account.info.code_hash.0), code.bytecode.to_vec()))
            } else {
                None
            }
        }));

        Self { db, journaled_state, factory_deps, override_keys }
    }

    /// Returns the nonce for a given account from NonceHolder storage.
    pub fn get_tx_nonce(&mut self, address: Address) -> Nonce {
        let address = address_to_h160(address);
        let nonce_key = get_nonce_key(&address);
        let nonce_storage = self.read_db(*nonce_key.address(), h256_to_u256(*nonce_key.key()));
        let (tx_nonce, _deploy_nonce) = decompose_full_nonce(h256_to_u256(nonce_storage));
        Nonce(tx_nonce.as_u32())
    }

    fn read_db(&mut self, address: H160, idx: U256) -> H256 {
        // let mut db = self.db.lock().unwrap();
        let addr = h160_to_address(address);
        self.journaled_state.load_account(addr, self.db).expect("failed loading account");
        let (r1, _) = self
            .journaled_state
            .sload(addr, u256_to_revm_u256(idx), self.db)
            .expect("failed sload");
        // let r2 = self.db.storage(addr, u256_to_revm_u256(idx)).ok().unwrap_or_default();
        // if r1 != r2 {
        //     // let a1 = self.journaled_state.load_account(addr, self.db);
        //     // let b1 = self.journaled_state.load_account(addr, self.db);
        //     panic!("{r1:?} != {r2:?} | {address:?} {idx:?}");
        // }

        revm_u256_to_h256(r1)

        // self.journaled_state
        //     .load_account(addr, self.db)
        //     .ok()
        //     .and_then(|(account, _)| {
        //         account.storage.get(&u256_to_revm_u256(idx)).map(|value| value.present_value)
        //     })
        //     .or_else(|| self.db.storage(addr, u256_to_revm_u256(idx)).ok())
        //     .map(revm_u256_to_h256)
        //     .unwrap_or_default()
    }
}

impl<'a, DB> ReadStorage for &mut ZKVMData<'a, DB>
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    fn read_value(&mut self, key: &StorageKey) -> zksync_types::StorageValue {
        self.read_db(*key.address(), h256_to_u256(*key.key()))
    }

    fn is_write_initial(&mut self, _key: &StorageKey) -> bool {
        false
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.factory_deps.get(&hash).cloned().or_else(|| {
            let result = self.db.code_by_hash(h256_to_b256(hash));
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
        })
    }

    fn get_enumeration_index(&mut self, _key: &StorageKey) -> Option<u64> {
        Some(0_u64)
    }
}
