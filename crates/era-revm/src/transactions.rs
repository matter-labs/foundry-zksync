use era_test_node::{
    fork::ForkDetails,
    node::{
        InMemoryNode, InMemoryNodeConfig, ShowCalls, ShowGasDetails, ShowStorageLogs, ShowVMDetails,
    },
    system_contracts,
};
use ethabi::ParamType;
use multivm::{interface::VmExecutionResultAndLogs, vm_refunds_enhancement::ToTracerPointer};
use revm::{
    primitives::{
        Account, AccountInfo, Address, Bytes, EVMResult, Env, Eval, Halt, HashMap as rHashMap,
        OutOfGasError, ResultAndState, StorageSlot, TxEnv, B256, KECCAK_EMPTY, U256 as rU256,
    },
    Database,
};
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    sync::{Arc, Mutex},
};
use zksync_basic_types::{web3::signing::keccak256, L1BatchNumber, L2ChainId, H160, H256, U256};
use zksync_types::api::Block;
use zksync_types::{
    fee::Fee, l2::L2Tx, transaction_request::PaymasterParams, PackedEthSignature, StorageKey,
    StorageLogQueryType, ACCOUNT_CODE_STORAGE_ADDRESS,
};

use revm::primitives::U256 as revmU256;
use zksync_utils::{h256_to_account_address, u256_to_h256};

use crate::{
    cheatcodes::CheatcodeTracer,
    conversion_utils::{
        address_to_h160, h160_to_address, h256_to_h160, h256_to_revm_u256, revm_u256_to_u256,
    },
    db::RevmDatabaseForEra,
    factory_deps::PackedEraBytecode,
};

fn contract_address_from_tx_result(execution_result: &VmExecutionResultAndLogs) -> Option<H160> {
    for query in execution_result.logs.storage_logs.iter().rev() {
        if query.log_type == StorageLogQueryType::InitialWrite
            && query.log_query.address == ACCOUNT_CODE_STORAGE_ADDRESS
        {
            return Some(h256_to_account_address(&u256_to_h256(query.log_query.key)));
        }
    }
    None
}

/// Prepares calldata to invoke deployer contract.
/// This method encodes parameters for the `create` method.
pub fn encode_deploy_params_create(
    salt: H256,
    contract_hash: H256,
    constructor_input: Vec<u8>,
) -> Vec<u8> {
    // TODO (SMA-1608): We should not re-implement the ABI parts in different places, instead have the ABI available
    //  from the `zksync_contracts` crate.
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
        // Currently zkSync doesn't allow gas limits larger than u32.
        gas_limit: U256::min(tx_env.gas_limit.into(), U256::from(2147483640)),
        // Block base fee on L2 is 0.25 GWei - make sure that the max_fee_per_gas is set to higher value.
        max_fee_per_gas: U256::max(revm_u256_to_u256(tx_env.gas_price), U256::from(260_000_000)),
        max_priority_fee_per_gas: revm_u256_to_u256(tx_env.gas_priority_fee.unwrap_or_default()),
        gas_per_pubdata_limit: U256::from(800),
    }
}

/// Translates Revm transaction into era's L2Tx.
pub fn tx_env_to_era_tx(tx_env: TxEnv, nonce: u64) -> L2Tx {
    let mut l2tx = match tx_env.transact_to {
        revm::primitives::TransactTo::Call(contract_address) => L2Tx::new(
            H160::from(contract_address.0 .0),
            tx_env.data.to_vec(),
            (tx_env.nonce.unwrap_or(nonce) as u32).into(),
            tx_env_to_fee(&tx_env),
            H160::from(tx_env.caller.0 .0),
            revm_u256_to_u256(tx_env.value),
            None, // factory_deps
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
    l2tx.set_input(
        tx_env.data.to_vec(),
        H256(keccak256(tx_env.data.to_vec().as_slice())),
    );
    l2tx
}

#[derive(Debug, Clone)]
pub enum DatabaseError {
    MissingCode(bool),
}

pub fn run_era_transaction<DB, E, INSP>(env: &mut Env, db: DB, _inspector: INSP) -> EVMResult<E>
where
    DB: Database + Send,
    <DB as revm::Database>::Error: Debug,
{
    let (num, ts) = (
        env.block.number.to::<u64>(),
        env.block.timestamp.to::<u64>(),
    );
    let era_db = RevmDatabaseForEra {
        db: Arc::new(Mutex::new(Box::new(db))),
        current_block: num,
    };

    let nonces = era_db.get_nonce_for_address(address_to_h160(env.tx.caller));

    println!(
        "*** Starting ERA transaction: block: {:?} timestamp: {:?} - but using {:?} and {:?} instead with nonce {:?}",
        env.block.number.to::<u32>(),
        env.block.timestamp.to::<u64>(),
        num,
        ts,
        nonces
    );

    // Update the environment timestamp and block number.
    // Check if this should be done at the end?
    env.block.number = env.block.number.saturating_add(rU256::from(1));
    env.block.timestamp = env.block.timestamp.saturating_add(rU256::from(1));

    let chain_id_u32 = if env.cfg.chain_id <= u32::MAX as u64 {
        env.cfg.chain_id as u32
    } else {
        // TODO: FIXME
        31337
    };

    let (l2_num, l2_ts) = (num * 2, ts * 2);
    let fork_details = ForkDetails {
        fork_source: &era_db,
        l1_block: L1BatchNumber(num as u32),
        l2_block: Block::default(),
        l2_miniblock: l2_num,
        l2_miniblock_hash: Default::default(),
        block_timestamp: l2_ts,
        overwrite_chain_id: Some(L2ChainId::from(chain_id_u32)),
        // Make sure that l1 gas price is set to reasonable values.
        l1_gas_price: u64::max(env.block.basefee.to::<u64>(), 1000),
    };

    let config = InMemoryNodeConfig {
        show_calls: ShowCalls::None,
        show_storage_logs: ShowStorageLogs::None,
        show_vm_details: ShowVMDetails::None,
        show_gas_details: ShowGasDetails::None,
        resolve_hashes: false,
        system_contracts_options: system_contracts::Options::BuiltInWithoutSecurity,
    };
    let node = InMemoryNode::new(Some(fork_details), None, config);

    let mut l2_tx = tx_env_to_era_tx(env.tx.clone(), nonces);

    if l2_tx.common_data.signature.is_empty() {
        // FIXME: This is a hack to make sure that the signature is not empty.
        // Fails without a signature here: https://github.com/matter-labs/zksync-era/blob/73a1e8ff564025d06e02c2689da238ae47bb10c3/core/lib/types/src/transaction_request.rs#L381
        l2_tx.common_data.signature = PackedEthSignature::default().serialize_packed().into();
    }

    let era_execution_result = node
        .run_l2_tx_raw(
            l2_tx,
            multivm::interface::TxExecutionMode::VerifyExecute,
            vec![CheatcodeTracer::new().into_tracer_pointer()],
        )
        .unwrap();

    let (modified_keys, tx_result, _call_traces, _block, bytecodes, _block_ctx) =
        era_execution_result;
    let maybe_contract_address = contract_address_from_tx_result(&tx_result);

    let execution_result = match tx_result.result {
        multivm::interface::ExecutionResult::Success { output, .. } => {
            revm::primitives::ExecutionResult::Success {
                reason: Eval::Return,
                gas_used: env.tx.gas_limit - tx_result.refunds.gas_refunded as u64,
                gas_refunded: tx_result.refunds.gas_refunded as u64,
                logs: vec![],
                output: revm::primitives::Output::Create(
                    Bytes::from(decode_l2_tx_result(output)),
                    maybe_contract_address.map(h160_to_address),
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
            // Need to decide what to do in the case of a halt. This might depend on the reason for the halt.
            // TODO: FIXME
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

    let account_to_keys: HashMap<H160, HashMap<StorageKey, H256>> =
        modified_keys
            .iter()
            .fold(HashMap::new(), |mut acc, (storage_key, value)| {
                acc.entry(*storage_key.address())
                    .or_default()
                    .insert(*storage_key, *value);
                acc
            });

    // List of touched accounts
    let mut accounts_touched: HashSet<H160> = Default::default();
    // All accounts where storage was modified.
    for x in account_to_keys.keys() {
        accounts_touched.insert(*x);
    }
    // Also insert 'fake' accounts for bytecodes (to make sure that factory bytecodes get persisted).
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

            let account_code = era_db.fetch_account_code(*account, &modified_keys, &bytecodes);

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

    Ok(ResultAndState {
        result: execution_result,
        state,
    })
}

fn decode_l2_tx_result(output: Vec<u8>) -> Vec<u8> {
    ethabi::decode(&[ParamType::Bytes], &output)
        .ok()
        .and_then(|result| result.first().cloned())
        .and_then(|result| result.into_bytes())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use crate::{factory_deps::hash_bytecode, testing::MockDatabase};

    use super::*;

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
            value: Default::default(),
            data: serde_json::to_vec(&PackedEraBytecode::new(
                hex::encode(hash_bytecode(&[0; 32])),
                hex::encode([0; 32]),
                vec![hex::encode([0; 32])],
            ))
            .unwrap()
            .into(),
            nonce: Default::default(),
            chain_id: Default::default(),
            access_list: Default::default(),
            gas_priority_fee: Default::default(),
            blob_hashes: Default::default(),
            max_fee_per_blob_gas: Default::default(),
        };

        let res =
            run_era_transaction::<_, ResultAndState, _>(&mut env, &mut MockDatabase::default(), ())
                .expect("failed executing");

        assert!(
            !res.state.is_empty(),
            "unexpected failure: no states were touched"
        );
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
