/// RevmDatabaseForEra allows era VM to use the revm "Database" object
/// as a read-only fork source.
/// This way, we can run transaction on top of the chain that is persisted
/// in the Database object.
/// This code doesn't do any mutatios to Database: after each transaction run, the Revm
/// is usually collecting all the diffs - and applies them to database itself.
use std::{collections::HashMap as sHashMap, fmt::Debug};

use alloy_primitives::{map::HashMap, Address, U256 as rU256};
use foundry_cheatcodes_common::record::RecordAccess;
use revm::{primitives::Account, Database, EvmContext, InnerEvmContext};
use zksync_basic_types::{L2ChainId, H160, H256, U256};
use zksync_state::interface::ReadStorage;
use zksync_types::{
    get_code_key, get_nonce_key, get_system_context_init_logs,
    utils::{decompose_full_nonce, storage_key_for_eth_balance},
    Nonce, StorageKey, StorageLog, StorageValue,
};

use zksync_utils::{bytecode::hash_bytecode, h256_to_u256};

use crate::convert::{ConvertAddress, ConvertH160, ConvertH256, ConvertRU256, ConvertU256};

/// Default chain id
pub(crate) const DEFAULT_CHAIN_ID: u32 = 31337;

pub struct ZKVMData<'a, DB: Database> {
    // pub db: &'a mut DB,
    // pub journaled_state: &'a mut JournaledState,
    ecx: &'a mut InnerEvmContext<DB>,
    pub factory_deps: HashMap<H256, Vec<u8>>,
    pub override_keys: sHashMap<StorageKey, StorageValue>,
    pub accesses: Option<&'a mut RecordAccess>,
}

impl<'a, DB> Debug for ZKVMData<'a, DB>
where
    DB: Database,
{
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
    pub fn new(ecx: &'a mut InnerEvmContext<DB>) -> Self {
        // load all deployed contract bytecodes from the JournaledState as factory deps
        let mut factory_deps = ecx
            .journaled_state
            .state
            .values()
            .flat_map(|account| {
                if account.info.is_empty_code_hash() {
                    None
                } else {
                    account.info.code.as_ref().map(|code| {
                        (H256::from(account.info.code_hash.0), code.bytecode().to_vec())
                    })
                }
            })
            .collect::<HashMap<_, _>>();

        let empty_code = vec![0u8; 32];
        let empty_code_hash = hash_bytecode(&empty_code);
        factory_deps.insert(empty_code_hash, empty_code);
        Self { ecx, factory_deps, override_keys: Default::default(), accesses: None }
    }

    /// Create a new instance of [ZKEVMData] with system contracts.
    pub fn new_with_system_contracts(ecx: &'a mut EvmContext<DB>, chain_id: L2ChainId) -> Self {
        let contracts = era_test_node::system_contracts::get_deployed_contracts(
            &era_test_node::system_contracts::Options::BuiltInWithoutSecurity,
            false,
        );
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
                (log.is_write()).then_some(override_keys.insert(log.key, log.value));
            });

        let mut factory_deps = contracts
            .into_iter()
            .map(|contract| (hash_bytecode(&contract.bytecode), contract.bytecode))
            .collect::<HashMap<_, _>>();
        factory_deps.extend(ecx.journaled_state.state.values().flat_map(|account| {
            if account.info.is_empty_code_hash() {
                None
            } else {
                account
                    .info
                    .code
                    .as_ref()
                    .map(|code| (H256::from(account.info.code_hash.0), code.bytecode().to_vec()))
            }
        }));
        let empty_code = vec![0u8; 32];
        let empty_code_hash = hash_bytecode(&empty_code);
        factory_deps.insert(empty_code_hash, empty_code);

        Self { ecx, factory_deps, override_keys, accesses: None }
    }

    /// Extends the currently known factory deps with the provided input
    pub fn with_extra_factory_deps(mut self, extra_factory_deps: HashMap<H256, Vec<u8>>) -> Self {
        self.factory_deps.extend(extra_factory_deps);
        self
    }

    /// Assigns the accesses coming from Foundry
    pub fn with_storage_accesses(mut self, accesses: Option<&'a mut RecordAccess>) -> Self {
        self.accesses = accesses;
        self
    }

    /// Returns the code hash for a given account from AccountCode storage.
    pub fn get_code_hash(&mut self, address: Address) -> H256 {
        let address = address.to_h160();
        let code_key = get_code_key(&address);
        self.read_db(*code_key.address(), h256_to_u256(*code_key.key()))
    }

    /// Returns the nonce for a given account from NonceHolder storage.
    pub fn get_tx_nonce(&mut self, address: Address) -> Nonce {
        let address = address.to_h160();
        let nonce_key = get_nonce_key(&address);
        let nonce_storage = self.read_db(*nonce_key.address(), h256_to_u256(*nonce_key.key()));
        let (tx_nonce, _deploy_nonce) = decompose_full_nonce(h256_to_u256(nonce_storage));
        Nonce(tx_nonce.as_u32())
    }

    /// Returns the deployment nonce for a given account from NonceHolder storage.
    pub fn get_deploy_nonce(&mut self, address: Address) -> Nonce {
        let address = address.to_h160();
        let nonce_key = get_nonce_key(&address);
        let nonce_storage = self.read_db(*nonce_key.address(), h256_to_u256(*nonce_key.key()));
        let (_tx_nonce, deploy_nonce) = decompose_full_nonce(h256_to_u256(nonce_storage));
        Nonce(deploy_nonce.as_u32())
    }

    /// Returns the nonce for a given account from NonceHolder storage.
    pub fn get_balance(&mut self, address: Address) -> U256 {
        let address = address.to_h160();
        let balance_key = storage_key_for_eth_balance(&address);
        let balance_storage =
            self.read_db(*balance_key.address(), h256_to_u256(*balance_key.key()));
        h256_to_u256(balance_storage)
    }

    /// Load an account into the journaled state.
    pub fn load_account(&mut self, address: Address) -> &mut Account {
        self.ecx.load_account(address).expect("account could not be loaded").data
    }

    /// Load an storage slot into the journaled state.
    /// The account must be already loaded else this function panics.
    pub fn sload(&mut self, address: Address, key: rU256) -> rU256 {
        self.ecx.sload(address, key).unwrap_or_default().data
    }

    fn read_db(&mut self, address: H160, idx: U256) -> H256 {
        let addr = address.to_address();
        self.ecx.load_account(addr).expect("failed loading account");
        self.ecx.sload(addr, idx.to_ru256()).expect("failed sload").to_h256()
    }
}

impl<'a, DB> ReadStorage for &mut ZKVMData<'a, DB>
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    fn read_value(&mut self, key: &StorageKey) -> zksync_types::StorageValue {
        if let Some(access) = &mut self.accesses {
            access.reads.entry(key.address().to_address()).or_default().push(key.key().to_ru256());
        }
        self.read_db(*key.address(), h256_to_u256(*key.key()))
    }

    fn is_write_initial(&mut self, _key: &StorageKey) -> bool {
        false
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.factory_deps.get(&hash).cloned().or_else(|| {
            let hash_b256 = hash.to_b256();
            self.ecx
                .journaled_state
                .state
                .values()
                .find_map(|account| {
                    if account.info.code_hash == hash_b256 {
                        return Some(account.info.code.clone().map(|code| code.bytecode().to_vec()))
                    }
                    None
                })
                .unwrap_or_else(|| {
                    self.ecx
                        .db
                        .code_by_hash(hash_b256)
                        .ok()
                        .map(|bytecode| bytecode.bytecode().to_vec())
                })
        })
    }

    fn get_enumeration_index(&mut self, _key: &StorageKey) -> Option<u64> {
        Some(0_u64)
    }
}
