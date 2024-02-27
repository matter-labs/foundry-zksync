use core::marker::PhantomData;
use ethers_core::abi::ethabi::{self, ParamType};
use itertools::Itertools;
use multivm::{
    interface::dyn_tracers::vm_1_4_0::DynTracer,
    vm_latest::{HistoryDisabled, HistoryMode, SimpleMemory, VmTracer},
};
use revm::primitives::{
    Account, AccountInfo, Address, Bytes, EVMResult, Env, Eval, Halt, HashMap as rHashMap,
    OutOfGasError, ResultAndState, StorageSlot, TxEnv, B256, KECCAK_EMPTY, U256 as rU256,
    U256 as revmU256,
};
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    sync::{Arc, Mutex},
};
use zksync_basic_types::{web3::signing::keccak256, L2ChainId, H160, H256, U256};
use zksync_state::WriteStorage;
use zksync_types::{
    fee::Fee, l2::L2Tx, transaction_request::PaymasterParams, PackedEthSignature, StorageKey,
    StorageValue, ACCOUNT_CODE_STORAGE_ADDRESS, CONTRACT_DEPLOYER_ADDRESS,
    KNOWN_CODES_STORAGE_ADDRESS,
};
use zksync_utils::{be_words_to_bytes, h256_to_account_address, h256_to_u256, u256_to_h256};

use foundry_common::{
    fix_l2_gas_limit, fix_l2_gas_price,
    zk_utils::{
        conversion_utils::{h160_to_address, h256_to_h160, h256_to_revm_u256, revm_u256_to_u256},
        factory_deps::PackedEraBytecode,
    },
    AsTracerPointer, StorageModificationRecorder, StorageModifications,
};

use super::db::RevmDatabaseForEra;
use crate::{
    backend::DatabaseExt,
    era_revm::{node::run_l2_tx_raw, storage_view::StorageView},
};

/// Prepares calldata to invoke deployer contract.
/// This method encodes parameters for the `create` method.
pub fn encode_deploy_params_create(
    salt: H256,
    contract_hash: H256,
    constructor_input: Vec<u8>,
) -> Vec<u8> {
    // TODO (SMA-1608): We should not re-implement the ABI parts in different places, instead have
    // the ABI available  from the `zksync_contracts` crate.
    let signature = ethabi::short_signature(
        "create",
        &[
            ethabi::ParamType::FixedBytes(32),
            ethabi::ParamType::FixedBytes(32),
            ethabi::ParamType::Bytes,
        ],
    );
    let params = ethabi::encode(&[
        ethabi::Token::FixedBytes(salt.as_bytes().to_vec()),
        ethabi::Token::FixedBytes(contract_hash.as_bytes().to_vec()),
        ethabi::Token::Bytes(constructor_input),
    ]);

    signature.iter().copied().chain(params).collect()
}

/// Extract the zkSync Fee based off the Revm transaction.
pub fn tx_env_to_fee(tx_env: &TxEnv) -> Fee {
    Fee {
        gas_limit: fix_l2_gas_limit(tx_env.gas_limit.into()),
        max_fee_per_gas: fix_l2_gas_price(revm_u256_to_u256(tx_env.gas_price)),
        max_priority_fee_per_gas: revm_u256_to_u256(tx_env.gas_priority_fee.unwrap_or_default()),
        gas_per_pubdata_limit: U256::from(800),
    }
}

/// Translates Revm transaction into era's L2Tx.
pub fn tx_env_to_era_tx(tx_env: TxEnv, nonce: u64, factory_deps: &HashMap<H256, Vec<u8>>) -> L2Tx {
    let factory_deps = if factory_deps.is_empty() {
        None
    } else {
        Some(factory_deps.values().cloned().collect_vec())
    };
    let mut l2tx = match tx_env.transact_to {
        revm::primitives::TransactTo::Call(contract_address) => L2Tx::new(
            H160::from(contract_address.0 .0),
            tx_env.data.to_vec(),
            (tx_env.nonce.unwrap_or(nonce) as u32).into(),
            tx_env_to_fee(&tx_env),
            H160::from(tx_env.caller.0 .0),
            revm_u256_to_u256(tx_env.value),
            factory_deps, // factory_deps
            PaymasterParams::default(),
        ),
        revm::primitives::TransactTo::Create(_scheme) => {
            // TODO: support create / create2.
            let packed_bytecode = PackedEraBytecode::from_vec(tx_env.data.as_ref());
            L2Tx::new(
                H160::from_low_u64_be(0x8006),
                encode_deploy_params_create(
                    Default::default(),
                    packed_bytecode.bytecode_hash(),
                    Default::default(),
                ),
                (tx_env.nonce.unwrap_or(nonce) as u32).into(),
                tx_env_to_fee(&tx_env),
                H160::from(tx_env.caller.0 .0),
                revm_u256_to_u256(tx_env.value),
                Some(packed_bytecode.factory_deps()),
                PaymasterParams::default(),
            )
        }
    };
    l2tx.set_input(tx_env.data.to_vec(), H256(keccak256(tx_env.data.to_vec().as_slice())));
    l2tx
}

#[derive(Debug, Clone)]
pub enum DatabaseError {
    MissingCode(bool),
}

pub fn run_era_transaction<DB, E, INSP>(env: &mut Env, db: DB, mut inspector: INSP) -> EVMResult<E>
where
    DB: DatabaseExt + Send,
    <DB as revm::Database>::Error: Debug,
    INSP: AsTracerPointer<StorageView<RevmDatabaseForEra<DB>>, HistoryDisabled>
        + StorageModificationRecorder,
{
    let mut era_db = RevmDatabaseForEra::new(Arc::new(Mutex::new(Box::new(db))));
    let (num, ts) = era_db.get_l2_block_number_and_timestamp();
    let l1_num = num;
    let nonce = era_db.get_nonce_for_address(H160::from_slice(env.tx.caller.as_slice()));

    info!(
        "Starting ERA transaction: block={:?} timestamp={:?} nonce={:?} | l1_block={}",
        num, ts, nonce, l1_num
    );

    // Update the environment timestamp and block number.
    // Check if this should be done at the end?
    // In general, we do not rely on env as it's consistently maintained in foundry
    env.block.number = env.block.number.saturating_add(rU256::from(1));
    env.block.timestamp = env.block.timestamp.saturating_add(rU256::from(1));

    let chain_id_u32 = if env.cfg.chain_id <= u32::MAX as u64 {
        env.cfg.chain_id as u32
    } else {
        // TODO: FIXME
        31337
    };

    let mut l2_tx =
        tx_env_to_era_tx(env.tx.clone(), nonce, &inspector.get_storage_modifications().bytecodes);

    if l2_tx.common_data.signature.is_empty() {
        // FIXME: This is a hack to make sure that the signature is not empty.
        // Fails without a signature here: https://github.com/matter-labs/zksync-era/blob/73a1e8ff564025d06e02c2689da238ae47bb10c3/core/lib/types/src/transaction_request.rs#L381
        l2_tx.common_data.signature = PackedEthSignature::default().serialize_packed().into();
    }
    let tracer = inspector.as_tracer_pointer();
    let storage = era_db.clone().into_storage_view_with_system_contracts(chain_id_u32);

    let storage_ptr = storage.into_rc_ptr();
    let (tx_result, bytecodes, modified_storage) = run_l2_tx_raw(
        l2_tx,
        storage_ptr.clone(),
        L2ChainId::from(chain_id_u32),
        u64::max(env.block.basefee.to::<u64>(), 1000),
        vec![tracer],
    );

    let modifications: &StorageModifications = inspector.get_storage_modifications();
    let mut known_codes = tx_result
        .logs
        .events
        .iter()
        .filter_map(|ev| {
            if ev.address == KNOWN_CODES_STORAGE_ADDRESS {
                let hash = ev.indexed_topics[1];
                let bytecode = bytecodes
                    .get(&h256_to_u256(hash))
                    .map(|bytecode| be_words_to_bytes(bytecode))
                    .expect("bytecode must exist");
                Some((hash, bytecode))
            } else {
                None
            }
        })
        .collect::<HashMap<_, _>>();

    // We need to track requested known_codes for scripting purposes
    // Any contracts deployed before will not be known, and thus
    // need their bytecode to be fetched from storage.
    // We exclude making the `load_factory_dep` when we already known the bytecide
    // to ensure forks are not queried for bytecodes, which can lead to "code should already be
    // loaded" errors.
    let requested_known_codes = storage_ptr
        .borrow()
        .read_storage_keys
        .iter()
        .filter_map(|(key, value)| {
            let hash = *key.key();
            if key.address() == &KNOWN_CODES_STORAGE_ADDRESS &&
                !value.is_zero() &&
                !bytecodes.contains_key(&h256_to_u256(hash)) &&
                !known_codes.contains_key(&hash) &&
                !modifications.bytecodes.contains_key(&hash) &&
                !modifications.known_codes.contains_key(&hash)
            {
                zksync_state::ReadStorage::load_factory_dep(&mut era_db, hash)
                    .map(|bytecode| (hash, bytecode))
            } else {
                None
            }
        })
        .collect::<HashMap<_, _>>();
    known_codes.extend(requested_known_codes);

    // Record storage modifications in the inspector.
    // We record known_codes only if they aren't already in the bytecodes changeset.
    inspector.record_storage_modifications(StorageModifications {
        keys: modified_storage.clone(),
        bytecodes: bytecodes
            .clone()
            .into_iter()
            .map(|(key, value)| {
                let key = u256_to_h256(key);
                let value = value
                    .into_iter()
                    .flat_map(|word| u256_to_h256(word).as_bytes().to_owned())
                    .collect_vec();
                (key, value)
            })
            .collect(),
        known_codes,
        deployed_codes: tx_result
            .logs
            .events
            .iter()
            .filter_map(|ev| {
                if ev.address == CONTRACT_DEPLOYER_ADDRESS {
                    let address = h256_to_h160(&ev.indexed_topics[3]);
                    let bytecode_hash = ev.indexed_topics[2];
                    Some((address, bytecode_hash))
                } else {
                    None
                }
            })
            .collect(),
    });

    let execution_result = match tx_result.result {
        multivm::interface::ExecutionResult::Success { output, .. } => {
            let logs = tx_result
                .logs
                .events
                .clone()
                .into_iter()
                .map(|event| revm::primitives::Log {
                    address: h160_to_address(event.address),
                    topics: event.indexed_topics.iter().cloned().map(|t| B256::from(t.0)).collect(),
                    data: event.value.into(),
                })
                .collect_vec();
            let result = decode_l2_tx_result(output);
            let address = if result.len() == 32 {
                Some(h256_to_account_address(&H256::from_slice(&result)))
            } else {
                None
            };
            revm::primitives::ExecutionResult::Success {
                reason: Eval::Return,
                gas_used: tx_result.statistics.gas_used as u64,
                gas_refunded: tx_result.refunds.gas_refunded as u64,
                logs,
                output: revm::primitives::Output::Create(
                    Bytes::from(result),
                    address.map(h160_to_address),
                ),
            }
        }
        multivm::interface::ExecutionResult::Revert { output } => {
            let output = match output {
                multivm::interface::VmRevertReason::General { data, .. } => data,
                multivm::interface::VmRevertReason::Unknown { data, .. } => data,
                _ => Vec::new(),
            };

            revm::primitives::ExecutionResult::Revert {
                gas_used: env.tx.gas_limit - tx_result.refunds.gas_refunded as u64,
                output: Bytes::from(output),
            }
        }
        multivm::interface::ExecutionResult::Halt { reason } => {
            // Need to decide what to do in the case of a halt. This might depend on the reason for
            // the halt. TODO: FIXME
            tracing::error!("tx execution halted: {}", reason);
            revm::primitives::ExecutionResult::Halt {
                reason: match reason {
                    multivm::interface::Halt::NotEnoughGasProvided => {
                        Halt::OutOfGas(OutOfGasError::BasicOutOfGas)
                    }
                    _ => panic!("HALT: {}", reason),
                },
                gas_used: env.tx.gas_limit - tx_result.refunds.gas_refunded as u64,
            }
        }
    };

    Ok(ResultAndState {
        result: execution_result,
        state: storage_to_state(&era_db, &modified_storage, bytecodes),
    })
}

fn decode_l2_tx_result(output: Vec<u8>) -> Vec<u8> {
    ethabi::decode(&[ParamType::Bytes], &output)
        .ok()
        .and_then(|result| result.first().cloned())
        .and_then(|result| result.into_bytes())
        .unwrap_or_default()
}

/// Converts the zksync era's modified keys to the revm state.
pub fn storage_to_state<DB>(
    era_db: &RevmDatabaseForEra<DB>,
    modified_keys: &HashMap<StorageKey, StorageValue>,
    bytecodes: HashMap<U256, Vec<U256>>,
) -> rHashMap<Address, Account>
where
    DB: DatabaseExt + Send,
    <DB as revm::Database>::Error: Debug,
{
    let account_to_keys: HashMap<H160, HashMap<StorageKey, H256>> =
        modified_keys.iter().fold(HashMap::new(), |mut acc, (storage_key, value)| {
            acc.entry(*storage_key.address()).or_default().insert(*storage_key, *value);
            acc
        });

    // List of touched accounts
    let mut accounts_touched: HashSet<H160> = Default::default();
    // All accounts where storage was modified.
    for x in account_to_keys.keys() {
        accounts_touched.insert(*x);
    }
    // Also insert 'fake' accounts for bytecodes (to make sure that factory bytecodes get
    // persisted).
    for k in bytecodes.keys() {
        accounts_touched.insert(h256_to_h160(&u256_to_h256(*k)));
    }

    let account_code_storage = ACCOUNT_CODE_STORAGE_ADDRESS;

    if let Some(account_bytecodes) = account_to_keys.get(&account_code_storage) {
        for k in account_bytecodes.keys() {
            let account_address = H160::from_slice(&k.key().0[12..32]);
            accounts_touched.insert(account_address);
        }
    }

    let state: rHashMap<Address, Account> = accounts_touched
        .iter()
        .map(|account| {
            let acc: Address = h160_to_address(*account);

            let storage: Option<rHashMap<revmU256, StorageSlot>> =
                account_to_keys.get(account).map(|slot_changes| {
                    slot_changes
                        .iter()
                        .map(|(slot, value)| {
                            (
                                h256_to_revm_u256(*slot.key()),
                                StorageSlot {
                                    previous_or_original_value: revm::primitives::U256::ZERO, // FIXME
                                    present_value: h256_to_revm_u256(*value),
                                },
                            )
                        })
                        .collect()
                });

            let account_code = era_db.fetch_account_code(*account, modified_keys, &bytecodes);

            let (code_hash, code) = account_code
                .map(|(hash, bytecode)| (B256::from(&hash.0), Some(bytecode)))
                .unwrap_or((KECCAK_EMPTY, None));
            if code.is_none() {
                println!("*** No bytecode for account: {:?}", account);
            }

            (
                acc,
                Account {
                    info: AccountInfo {
                        balance: revm::primitives::U256::ZERO, // FIXME
                        nonce: era_db.get_nonce_for_address(*account),
                        code_hash,
                        code,
                    },
                    storage: storage.unwrap_or_default(),
                    status: revm::primitives::AccountStatus::Touched,
                },
            )
        })
        .collect();
    state
}

pub struct NoopEraInspector<S, H> {
    _phantom: PhantomData<(S, H)>,
    storage_modifications: StorageModifications,
}

impl<S, H> Default for NoopEraInspector<S, H> {
    fn default() -> Self {
        Self { _phantom: Default::default(), storage_modifications: Default::default() }
    }
}

impl<S, H> Clone for NoopEraInspector<S, H> {
    fn clone(&self) -> Self {
        Self { _phantom: self._phantom, storage_modifications: self.storage_modifications.clone() }
    }
}

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for NoopEraInspector<S, H> {}
impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for NoopEraInspector<S, H> {}
impl<S: WriteStorage + 'static, H: HistoryMode + 'static> AsTracerPointer<S, H>
    for NoopEraInspector<S, H>
{
    fn as_tracer_pointer(&self) -> multivm::vm_latest::TracerPointer<S, H> {
        Box::new(self.clone())
    }
}

impl<S, H> StorageModificationRecorder for NoopEraInspector<S, H> {
    fn record_storage_modifications(&mut self, _storage_modifications: StorageModifications) {}

    fn get_storage_modifications(&self) -> &StorageModifications {
        &self.storage_modifications
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::era_revm::testing::MockDatabase;
    use zksync_utils::bytecode::hash_bytecode;

    #[test]
    fn test_env_number_and_timestamp_is_incremented_after_transaction_and_marks_storage_as_touched()
    {
        let mut env = Env::default();

        env.block.number = rU256::from(0);
        env.block.timestamp = rU256::from(0);

        env.tx = TxEnv {
            caller: Address(H160::repeat_byte(0x1).to_fixed_bytes().into()),
            gas_limit: 1_000_000,
            gas_price: rU256::from(250_000_000),
            transact_to: revm::primitives::TransactTo::Create(
                revm::primitives::CreateScheme::Create,
            ),
            data: serde_json::to_vec(&PackedEraBytecode::new(
                hex::encode(hash_bytecode(&[0; 32])),
                hex::encode([0; 32]),
                vec![hex::encode([0; 32])],
            ))
            .unwrap()
            .into(),
            ..Default::default()
        };
        let mock_db = MockDatabase::default();

        let res = run_era_transaction::<_, ResultAndState, _>(
            &mut env,
            mock_db,
            NoopEraInspector::default(),
        )
        .expect("failed executing");

        assert!(!res.state.is_empty(), "unexpected failure: no states were touched");
        for (address, account) in res.state {
            assert!(
                account.is_touched(),
                "unexpected failure:  account {} was not marked as touched; it will not be updated",
                address
            );
        }

        assert_eq!(1, env.block.number.to::<u64>());
        assert_eq!(1, env.block.timestamp.to::<u64>());
    }
}
