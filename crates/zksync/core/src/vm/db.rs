/// RevmDatabaseForEra allows era VM to use the revm "Database" object
/// as a read-only fork source.
/// This way, we can run transaction on top of the chain that is persisted
/// in the Database object.
/// This code doesn't do any mutatios to Database: after each transaction run, the Revm
/// is usually collecting all the diffs - and applies them to database itself.
use std::{collections::HashMap as sHashMap, fmt::Debug, sync::LazyLock};

use alloy_evm::eth::EthEvmContext;
use alloy_primitives::{Address, U256 as rU256, map::HashMap};
use foundry_cheatcodes_common::record::RecordAccess;
use revm::{Database, context::JournalTr, state::Account};
use zksync_basic_types::{H160, H256, L2ChainId, U256};
use zksync_types::{
    CREATE2_FACTORY_ADDRESS, StorageKey, StorageLog, StorageValue, get_code_key, get_nonce_key,
    get_system_context_init_logs, h256_to_u256,
    utils::{decompose_full_nonce, storage_key_for_eth_balance},
};
use zksync_vm_interface::storage::ReadStorage;

use crate::{
    DEFAULT_PROTOCOL_VERSION,
    convert::{ConvertAddress, ConvertH160, ConvertH256, ConvertRU256, ConvertU256},
    hash_bytecode,
    state::FullNonce,
};
use anvil_zksync_core::deps::system_contracts::NON_KERNEL_CONTRACT_LOCATIONS;

use super::storage_recorder::{AccountAccess, AccountAccesses, CallType, StorageAccessRecorder};

/// Default chain id
pub(crate) const DEFAULT_CHAIN_ID: u32 = 31337;

// NOTE: we use vec instead of hashmap because the loaded [BOOTLOADER] and [0 address] share the
// same bytecode, thus they would share the same bytecode hash (key in map) resulting in the first
// `DeployedContract` to be discarded from the resulting map. This is a problem when we compute the
// override keys as the discarded contract won't have an associated generated override
// TL;DR: we want to keep _all_ instances of `DeployedContract` even if they share the same bytecode
// hash
struct DeployedSystemContract {
    deployed_contract: zksync_types::block::DeployedContract,
    deployed_contract_hash: H256,
}

static DEPLOYED_SYSTEM_CONTRACTS: LazyLock<Vec<DeployedSystemContract>> = LazyLock::new(|| {
    let contracts = anvil_zksync_core::deps::system_contracts::get_deployed_contracts(
        anvil_zksync_config::types::SystemContractsOptions::BuiltInWithoutSecurity,
        DEFAULT_PROTOCOL_VERSION,
        None,
    );

    let filtered = contracts.into_iter().filter(|contract| {
        let addr = contract.account_id.address();

        if *addr == CREATE2_FACTORY_ADDRESS {
            return true;
        }

        // Drop anything that matches a non-kernel contract location.
        !NON_KERNEL_CONTRACT_LOCATIONS.iter().any(|(_name, a, _ver)| *a == *addr)
    });

    filtered
        .map(|contract| DeployedSystemContract {
            deployed_contract_hash: hash_bytecode(&contract.bytecode),
            deployed_contract: contract,
        })
        .collect()
});
pub struct ZKVMData<'a, DB: Database> {
    ecx: &'a mut EthEvmContext<DB>,
    pub factory_deps: HashMap<H256, Vec<u8>>,
    pub override_keys: sHashMap<StorageKey, StorageValue>,
    pub accesses: Option<&'a mut RecordAccess>,
    pub account_accesses: AccountAccesses,
}

impl<DB> Debug for ZKVMData<'_, DB>
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
    pub fn new(ecx: &'a mut EthEvmContext<DB>) -> Self {
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
                        (H256::from(account.info.code_hash.0), code.original_bytes().to_vec())
                    })
                }
            })
            .collect::<HashMap<_, _>>();

        let empty_code = vec![0u8; 32];
        let empty_code_hash = hash_bytecode(&empty_code);
        factory_deps.insert(empty_code_hash, empty_code);
        Self {
            ecx,
            factory_deps,
            override_keys: Default::default(),
            accesses: None,
            account_accesses: Default::default(),
        }
    }

    /// Create a new instance of [ZKEVMData] with system contracts.
    pub fn new_with_system_contracts(ecx: &'a mut EthEvmContext<DB>, chain_id: L2ChainId) -> Self {
        let system_context_init_log = get_system_context_init_logs(chain_id);

        let mut override_keys = HashMap::default();
        DEPLOYED_SYSTEM_CONTRACTS
            .iter()
            .map(|c| {
                let deployer_code_key = get_code_key(c.deployed_contract.account_id.address());
                StorageLog::new_write_log(deployer_code_key, c.deployed_contract_hash)
            })
            .chain(system_context_init_log)
            .for_each(|log| {
                (log.is_write()).then_some(override_keys.insert(log.key, log.value));
            });

        let system_factory_deps = DEPLOYED_SYSTEM_CONTRACTS
            .iter()
            .map(|c| (c.deployed_contract_hash, c.deployed_contract.bytecode.clone()));

        let state_to_factory_deps = ecx.journaled_state.state.values().flat_map(|account| {
            if account.info.is_empty_code_hash() {
                None
            } else {
                account.info.code.as_ref().map(|code| {
                    (H256::from(account.info.code_hash.0), code.original_bytes().to_vec())
                })
            }
        });

        let empty_code = vec![0u8; 32];
        let empty_code_hash = hash_bytecode(&empty_code);

        let factory_deps = system_factory_deps
            .chain(state_to_factory_deps)
            .chain([(empty_code_hash, empty_code)])
            .collect();

        Self {
            ecx,
            factory_deps,
            override_keys,
            accesses: None,
            account_accesses: Default::default(),
        }
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

    /// Returns the [FullNonce] for a given account from NonceHolder storage.
    pub fn get_full_nonce(&mut self, address: Address) -> FullNonce {
        let address = address.to_h160();
        let nonce_key = get_nonce_key(&address);
        let nonce_storage = self.read_db(*nonce_key.address(), h256_to_u256(*nonce_key.key()));
        let (tx_nonce, deploy_nonce) = decompose_full_nonce(h256_to_u256(nonce_storage));
        FullNonce { tx_nonce: tx_nonce.as_u128(), deploy_nonce: deploy_nonce.as_u128() }
    }

    /// Returns the nonce for a given account from NonceHolder storage.
    pub fn get_tx_nonce(&mut self, address: Address) -> u128 {
        let address = address.to_h160();
        let nonce_key = get_nonce_key(&address);
        let nonce_storage = self.read_db(*nonce_key.address(), h256_to_u256(*nonce_key.key()));
        let (tx_nonce, _deploy_nonce) = decompose_full_nonce(h256_to_u256(nonce_storage));
        tx_nonce.as_u128()
    }

    /// Returns the deployment nonce for a given account from NonceHolder storage.
    pub fn get_deploy_nonce(&mut self, address: Address) -> u128 {
        let address = address.to_h160();
        let nonce_key = get_nonce_key(&address);
        let nonce_storage = self.read_db(*nonce_key.address(), h256_to_u256(*nonce_key.key()));
        let (_tx_nonce, deploy_nonce) = decompose_full_nonce(h256_to_u256(nonce_storage));
        deploy_nonce.as_u128()
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
        self.ecx.journaled_state.load_account(address).expect("account could not be loaded").data
    }

    /// Load an storage slot into the journaled state.
    /// The account must be already loaded else this function panics.
    pub fn sload(&mut self, address: Address, key: rU256) -> rU256 {
        self.ecx.journaled_state.sload(address, key).unwrap_or_default().data
    }

    pub fn get_account_accesses(&mut self) -> Vec<AccountAccess> {
        std::mem::take(&mut self.account_accesses).get_records()
    }

    fn read_db(&mut self, address: H160, idx: U256) -> H256 {
        let addr = address.to_address();
        self.ecx.journaled_state.load_account(addr).expect("failed loading account");
        self.ecx.journaled_state.sload(addr, idx.to_ru256()).expect("failed sload").to_h256()
    }
}

impl<DB> StorageAccessRecorder for &mut ZKVMData<'_, DB>
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    fn start_recording(&mut self) {
        self.account_accesses.start_recording();
    }

    fn stop_recording(&mut self) {
        self.account_accesses.stop_recording();
    }

    fn record_read(&mut self, key: &StorageKey, value: H256) {
        self.account_accesses.record_read(key, value);
    }

    fn record_write(&mut self, key: &StorageKey, old_value: H256, new_value: H256) {
        self.account_accesses.record_write(key, old_value, new_value);
    }

    fn record_call_start(
        &mut self,
        call_type: CallType,
        accessor: Address,
        account: Address,
        balance: rU256,
        data: Vec<u8>,
        value: rU256,
    ) {
        self.account_accesses.record_call_start(call_type, accessor, account, balance, data, value);
    }

    fn record_call_end(&mut self, accessor: Address, account: Address, new_balance: rU256) {
        self.account_accesses.record_call_end(accessor, account, new_balance);
    }
}

impl<DB> ReadStorage for &mut ZKVMData<'_, DB>
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    fn read_value(&mut self, key: &StorageKey) -> zksync_types::StorageValue {
        if let Some(access) = &mut self.accesses {
            access.reads.entry(key.address().to_address()).or_default().push(key.key().to_ru256());
        }

        let value = self.read_db(*key.address(), h256_to_u256(*key.key()));
        self.record_read(key, value);
        value
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
                        return Some(
                            account.info.code.clone().map(|code| code.original_bytes().to_vec()),
                        );
                    }
                    None
                })
                .unwrap_or_else(|| {
                    self.ecx
                        .journaled_state
                        .db()
                        .code_by_hash(hash_b256)
                        .ok()
                        .map(|bytecode| bytecode.original_bytes().to_vec())
                })
        })
    }

    fn get_enumeration_index(&mut self, _key: &StorageKey) -> Option<u64> {
        Some(0_u64)
    }
}
