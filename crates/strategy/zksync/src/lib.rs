use std::{
    collections::hash_map::Entry,
    sync::{Arc, Mutex},
};

use alloy_primitives::{keccak256, Address, Bytes, B256, U256};
use alloy_sol_types::SolValue;
use foundry_evm_core::{
    backend::{
        strategy::{
            merge_db_account_data, merge_journaled_state_data, BackendStrategy,
            BackendStrategyForkInfo, CheatcodeInspectorStrategy, EvmBackendStrategy,
            ExecutorStrategy, GlobalStrategy,
        },
        BackendInner, DatabaseExt, Fork, ForkDB, FoundryEvmInMemoryDB,
    },
    constants::{CHEATCODE_ADDRESS, CHEATCODE_CONTRACT_HASH},
    InspectorExt,
};
use foundry_strategy_core::RunnerStrategy;
use foundry_zksync_compiler::{DualCompiledContract, DualCompiledContracts};
use foundry_zksync_core::{
    convert::ConvertH160, PaymasterParams, ZkPaymasterData, ACCOUNT_CODE_STORAGE_ADDRESS, H256,
    IMMUTABLE_SIMULATOR_STORAGE_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS, L2_BASE_TOKEN_ADDRESS,
    NONCE_HOLDER_ADDRESS,
};
use revm::{
    db::CacheDB,
    primitives::{Bytecode, EnvWithHandlerCfg, HashMap, HashSet, ResultAndState},
    DatabaseRef, JournaledState,
};
use serde::{Deserialize, Serialize};
use tracing::trace;

#[derive(Debug, Default, Clone)]
pub struct ZksyncStrategy;

impl GlobalStrategy for ZksyncStrategy {
    type Backend = ZkBackendStrategy;
    type Executor = ZkExecutor;
    type CheatcodeInspector = ZkCheatcodeInspector;
}

#[derive(Debug, Default, Clone)]
pub struct ZkExecutor;
impl ExecutorStrategy for ZkExecutor {
    fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::default()))
    }
}

#[derive(Debug, Default, Clone)]
pub struct ZkCheatcodeInspector {
    /// When in zkEVM context, execute the next CALL or CREATE in the EVM instead.
    pub skip_zk_vm: bool,

    /// Any contracts that were deployed in `skip_zk_vm` step.
    /// This makes it easier to dispatch calls to any of these addresses in zkEVM context, directly
    /// to EVM. Alternatively, we'd need to add `vm.zkVmSkip()` to these calls manually.
    pub skip_zk_vm_addresses: HashSet<Address>,

    /// Records the next create address for `skip_zk_vm_addresses`.
    pub record_next_create_address: bool,

    /// Paymaster params
    pub paymaster_params: Option<ZkPaymasterData>,

    /// Dual compiled contracts
    pub dual_compiled_contracts: DualCompiledContracts,

    /// The migration status of the database to zkEVM storage, `None` if we start in EVM context.
    pub zk_startup_migration: ZkStartupMigration,

    /// Factory deps stored through `zkUseFactoryDep`. These factory deps are used in the next
    /// CREATE or CALL, and cleared after.
    pub zk_use_factory_deps: Vec<String>,

    /// The list of factory_deps seen so far during a test or script execution.
    /// Ideally these would be persisted in the storage, but since modifying [revm::JournaledState]
    /// would be a significant refactor, we maintain the factory_dep part in the [Cheatcodes].
    /// This can be done as each test runs with its own [Cheatcodes] instance, thereby
    /// providing the necessary level of isolation.
    pub persisted_factory_deps: HashMap<H256, Vec<u8>>,

    /// Nonce update persistence behavior in zkEVM for the tx caller.
    pub zk_persist_nonce_update: ZkPersistNonceUpdate,
}

/// Allows overriding nonce update behavior for the tx caller in the zkEVM.
///
/// Since each CREATE or CALL is executed as a separate transaction within zkEVM, we currently skip
/// persisting nonce updates as it erroneously increments the tx nonce. However, under certain
/// situations, e.g. deploying contracts, transacts, etc. the nonce updates must be persisted.
#[derive(Default, Debug, Clone)]
pub enum ZkPersistNonceUpdate {
    /// Never update the nonce. This is currently the default behavior.
    #[default]
    Never,
    /// Override the default behavior, and persist nonce update for tx caller for the next
    /// zkEVM execution _only_.
    PersistNext,
}

impl ZkPersistNonceUpdate {
    /// Persist nonce update for the tx caller for next execution.
    pub fn persist_next(&mut self) {
        *self = Self::PersistNext;
    }

    /// Retrieve if a nonce update must be persisted, or not. Resets the state to default.
    pub fn check(&mut self) -> bool {
        let persist_nonce_update = match self {
            Self::Never => false,
            Self::PersistNext => true,
        };
        *self = Default::default();

        persist_nonce_update
    }
}

impl CheatcodeInspectorStrategy for ZkCheatcodeInspector {
    fn initialize(&mut self, mut dual_compiled_contracts: DualCompiledContracts) {
        // We add the empty bytecode manually so it is correctly translated in zk mode.
        // This is used in many places in foundry, e.g. in cheatcode contract's account code.
        let empty_bytes = Bytes::from_static(&[0]);
        let zk_bytecode_hash = foundry_zksync_core::hash_bytecode(&foundry_zksync_core::EMPTY_CODE);
        let zk_deployed_bytecode = foundry_zksync_core::EMPTY_CODE.to_vec();

        dual_compiled_contracts.push(DualCompiledContract {
            name: String::from("EmptyEVMBytecode"),
            zk_bytecode_hash,
            zk_deployed_bytecode: zk_deployed_bytecode.clone(),
            zk_factory_deps: Default::default(),
            evm_bytecode_hash: B256::from_slice(&keccak256(&empty_bytes)[..]),
            evm_deployed_bytecode: Bytecode::new_raw(empty_bytes.clone()).bytecode().to_vec(),
            evm_bytecode: Bytecode::new_raw(empty_bytes).bytecode().to_vec(),
        });

        let cheatcodes_bytecode = {
            let mut bytecode = CHEATCODE_ADDRESS.abi_encode_packed();
            bytecode.append(&mut [0; 12].to_vec());
            Bytes::from(bytecode)
        };
        dual_compiled_contracts.push(DualCompiledContract {
            name: String::from("CheatcodeBytecode"),
            // we put a different bytecode hash here so when importing back to EVM
            // we avoid collision with EmptyEVMBytecode for the cheatcodes
            zk_bytecode_hash: foundry_zksync_core::hash_bytecode(CHEATCODE_CONTRACT_HASH.as_ref()),
            zk_deployed_bytecode: cheatcodes_bytecode.to_vec(),
            zk_factory_deps: Default::default(),
            evm_bytecode_hash: CHEATCODE_CONTRACT_HASH,
            evm_deployed_bytecode: cheatcodes_bytecode.to_vec(),
            evm_bytecode: cheatcodes_bytecode.to_vec(),
        });

        let mut persisted_factory_deps = HashMap::new();
        persisted_factory_deps.insert(zk_bytecode_hash, zk_deployed_bytecode);

        self.zk_startup_migration = ZkStartupMigration::Defer;
    }
}

/// Setting for migrating the database to zkEVM storage when starting in ZKsync mode.
/// The migration is performed on the DB via the inspector so must only be performed once.
#[derive(Debug, Default, Clone)]
pub enum ZkStartupMigration {
    /// Defer database migration to a later execution point.
    ///
    /// This is required as we need to wait for some baseline deployments
    /// to occur before the test/script execution is performed.
    #[default]
    Defer,
    /// Allow database migration.
    Allow,
    /// Database migration has already been performed.
    Done,
}

impl ZkStartupMigration {
    /// Check if startup migration is allowed. Migration is disallowed if it's to be deferred or has
    /// already been performed.
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }

    /// Allow migrating the the DB to zkEVM storage.
    pub fn allow(&mut self) {
        *self = Self::Allow
    }

    /// Mark the migration as completed. It must not be performed again.
    pub fn done(&mut self) {
        *self = Self::Done
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ZkBackendStrategy {
    evm: EvmBackendStrategy,
    persisted_factory_deps: HashMap<H256, Vec<u8>>,
    persistent_immutable_keys: HashMap<Address, HashSet<U256>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ZkBackendInspectData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub factory_deps: Option<Vec<Vec<u8>>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub paymaster_data: Option<PaymasterParams>,

    pub use_evm: bool,
}

impl BackendStrategy for ZkBackendStrategy {
    fn name(&self) -> &'static str {
        "zk"
    }

    /// When creating or switching forks, we update the AccountInfo of the contract.
    fn update_fork_db(
        &self,
        fork_info: BackendStrategyForkInfo<'_>,
        mem_db: &FoundryEvmInMemoryDB,
        backend_inner: &BackendInner,
        active_journaled_state: &mut JournaledState,
        target_fork: &mut Fork,
    ) {
        self.update_fork_db_contracts(
            fork_info,
            mem_db,
            backend_inner,
            active_journaled_state,
            target_fork,
        )
    }

    fn inspect<'i, 'db, I: InspectorExt>(
        &mut self,
        db: &'db mut dyn DatabaseExt,
        env: &mut EnvWithHandlerCfg,
        inspector: &'i mut I,
        extra: Option<Vec<u8>>,
    ) -> eyre::Result<ResultAndState> {
        let zk_extra = extra
            .as_ref()
            .map(|bytes| {
                serde_json::from_slice::<'_, ZkBackendInspectData>(&bytes).unwrap_or_default()
            })
            .unwrap_or_default();

        if zk_extra.use_evm {
            return self.evm.inspect(db, env, inspector, extra);
        }

        db.initialize(env);
        foundry_zksync_core::vm::transact(
            Some(&mut self.persisted_factory_deps),
            zk_extra.factory_deps,
            zk_extra.paymaster_data,
            env,
            db,
        )
    }
}

impl ZkBackendStrategy {
    /// Merges the state of all `accounts` from the currently active db into the given `fork`
    pub(crate) fn update_fork_db_contracts(
        &self,
        fork_info: BackendStrategyForkInfo<'_>,
        mem_db: &FoundryEvmInMemoryDB,
        backend_inner: &BackendInner,
        active_journaled_state: &mut JournaledState,
        target_fork: &mut Fork,
    ) {
        let _require_zk_storage_merge =
            fork_info.active_type.is_zk() && fork_info.target_type.is_zk();

        // Ignore EVM interoperatability and import everything
        // if !require_zk_storage_merge {
        //     return;
        // }

        let accounts = backend_inner.persistent_accounts.iter().copied();
        let zk_state = &ZkMergeState { persistent_immutable_keys: &self.persistent_immutable_keys };
        if let Some(db) = fork_info.active_fork.map(|f| &f.db) {
            ZkBackendMergeStrategy::merge_account_data(
                accounts,
                db,
                active_journaled_state,
                target_fork,
                zk_state,
            )
        } else {
            ZkBackendMergeStrategy::merge_account_data(
                accounts,
                mem_db,
                active_journaled_state,
                target_fork,
                zk_state,
            )
        }
    }
}

pub(crate) struct ZkBackendMergeStrategy;

/// Defines the zksync specific state to help during merge.
pub(crate) struct ZkMergeState<'a> {
    persistent_immutable_keys: &'a HashMap<Address, HashSet<U256>>,
}

impl ZkBackendMergeStrategy {
    /// Clones the data of the given `accounts` from the `active` database into the `fork_db`
    /// This includes the data held in storage (`CacheDB`) and kept in the `JournaledState`.
    pub fn merge_account_data<ExtDB: DatabaseRef>(
        accounts: impl IntoIterator<Item = Address>,
        active: &CacheDB<ExtDB>,
        active_journaled_state: &mut JournaledState,
        target_fork: &mut Fork,
        zk_state: &ZkMergeState<'_>,
    ) {
        for addr in accounts.into_iter() {
            merge_db_account_data(addr, active, &mut target_fork.db);
            merge_zk_account_data(addr, active, &mut target_fork.db, zk_state);
            merge_journaled_state_data(
                addr,
                active_journaled_state,
                &mut target_fork.journaled_state,
            );
            merge_zk_journaled_state_data(
                addr,
                active_journaled_state,
                &mut target_fork.journaled_state,
                zk_state,
            );
        }

        // need to mock empty journal entries in case the current checkpoint is higher than the
        // existing journal entries
        while active_journaled_state.journal.len() > target_fork.journaled_state.journal.len() {
            target_fork.journaled_state.journal.push(Default::default());
        }

        *active_journaled_state = target_fork.journaled_state.clone();
    }
}

/// Clones the zk account data from the `active` db into the `ForkDB`
fn merge_zk_account_data<ExtDB: DatabaseRef>(
    addr: Address,
    active: &CacheDB<ExtDB>,
    fork_db: &mut ForkDB,
    _zk_state: &ZkMergeState<'_>,
) {
    let merge_system_contract_entry =
        |fork_db: &mut ForkDB, system_contract: Address, slot: U256| {
            let Some(acc) = active.accounts.get(&system_contract) else { return };

            // port contract cache over
            if let Some(code) = active.contracts.get(&acc.info.code_hash) {
                trace!("merging contract cache");
                fork_db.contracts.insert(acc.info.code_hash, code.clone());
            }

            // prepare only the specified slot in account storage
            let mut new_acc = acc.clone();
            new_acc.storage = Default::default();
            if let Some(value) = acc.storage.get(&slot) {
                new_acc.storage.insert(slot, *value);
            }

            // port account storage over
            match fork_db.accounts.entry(system_contract) {
                Entry::Vacant(vacant) => {
                    trace!("target account not present - inserting from active");
                    // if the fork_db doesn't have the target account
                    // insert the entire thing
                    vacant.insert(new_acc);
                }
                Entry::Occupied(mut occupied) => {
                    trace!("target account present - merging storage slots");
                    // if the fork_db does have the system,
                    // extend the existing storage (overriding)
                    let fork_account = occupied.get_mut();
                    fork_account.storage.extend(&new_acc.storage);
                }
            }
        };

    merge_system_contract_entry(
        fork_db,
        L2_BASE_TOKEN_ADDRESS.to_address(),
        foundry_zksync_core::get_balance_key(addr),
    );
    merge_system_contract_entry(
        fork_db,
        ACCOUNT_CODE_STORAGE_ADDRESS.to_address(),
        foundry_zksync_core::get_account_code_key(addr),
    );
    merge_system_contract_entry(
        fork_db,
        NONCE_HOLDER_ADDRESS.to_address(),
        foundry_zksync_core::get_nonce_key(addr),
    );

    if let Some(acc) = active.accounts.get(&addr) {
        merge_system_contract_entry(
            fork_db,
            KNOWN_CODES_STORAGE_ADDRESS.to_address(),
            U256::from_be_slice(&acc.info.code_hash.0[..]),
        );
    }
}

/// Clones the account data from the `active_journaled_state` into the `fork_journaled_state` for
/// zksync storage.
fn merge_zk_journaled_state_data(
    addr: Address,
    active_journaled_state: &JournaledState,
    fork_journaled_state: &mut JournaledState,
    zk_state: &ZkMergeState<'_>,
) {
    let merge_system_contract_entry =
        |fork_journaled_state: &mut JournaledState, system_contract: Address, slot: U256| {
            if let Some(acc) = active_journaled_state.state.get(&system_contract) {
                // prepare only the specified slot in account storage
                let mut new_acc = acc.clone();
                new_acc.storage = Default::default();
                if let Some(value) = acc.storage.get(&slot).cloned() {
                    new_acc.storage.insert(slot, value);
                }

                match fork_journaled_state.state.entry(system_contract) {
                    Entry::Vacant(vacant) => {
                        vacant.insert(new_acc);
                    }
                    Entry::Occupied(mut occupied) => {
                        let fork_account = occupied.get_mut();
                        fork_account.storage.extend(new_acc.storage);
                    }
                }
            }
        };

    merge_system_contract_entry(
        fork_journaled_state,
        L2_BASE_TOKEN_ADDRESS.to_address(),
        foundry_zksync_core::get_balance_key(addr),
    );
    merge_system_contract_entry(
        fork_journaled_state,
        ACCOUNT_CODE_STORAGE_ADDRESS.to_address(),
        foundry_zksync_core::get_account_code_key(addr),
    );
    merge_system_contract_entry(
        fork_journaled_state,
        NONCE_HOLDER_ADDRESS.to_address(),
        foundry_zksync_core::get_nonce_key(addr),
    );

    if let Some(acc) = active_journaled_state.state.get(&addr) {
        merge_system_contract_entry(
            fork_journaled_state,
            KNOWN_CODES_STORAGE_ADDRESS.to_address(),
            U256::from_be_slice(&acc.info.code_hash.0[..]),
        );
    }

    // merge immutable storage.
    let immutable_simulator_addr = IMMUTABLE_SIMULATOR_STORAGE_ADDRESS.to_address();
    if let Some(immutable_storage_keys) = zk_state.persistent_immutable_keys.get(&addr) {
        for slot_key in immutable_storage_keys {
            merge_system_contract_entry(fork_journaled_state, immutable_simulator_addr, *slot_key);
        }
    }
}

pub struct ZkRunnerStrategy {
    pub backend: Arc<Mutex<ZkBackendStrategy>>,
}
impl Default for ZkRunnerStrategy {
    fn default() -> Self {
        Self { backend: Arc::new(Mutex::new(ZkBackendStrategy::default())) }
    }
}
impl RunnerStrategy for ZkRunnerStrategy {
    fn name(&self) -> &'static str {
        "zk"
    }

    fn backend_strategy(&self) -> Arc<Mutex<impl BackendStrategy>> {
        self.backend.clone()
    }
}
