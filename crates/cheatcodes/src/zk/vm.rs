use std::{collections::HashMap, fmt::Debug, str::FromStr, sync::Arc};

use era_test_node::{
    console_log::ConsoleLogHandler,
    formatter,
    node::ShowCalls,
    system_contracts::{Options, SystemContracts},
    utils::bytecode_to_factory_dep,
};
use foundry_common::{
    conversion_utils::address_to_h160,
    fix_l2_gas_limit, fix_l2_gas_price,
    zk_utils::conversion_utils::{
        h160_to_address, h256_to_h160, h256_to_revm_u256, revm_u256_to_u256,
    },
    DualCompiledContract,
};
use itertools::Itertools;
use multivm::{
    interface::{Halt, VmExecutionResultAndLogs, VmInterface, VmRevertReason},
    tracers::CallTracer,
    vm_latest::{ExecutionResult, HistoryDisabled, ToTracerPointer, Vm, VmExecutionMode},
};
use once_cell::sync::OnceCell;
use revm::{
    interpreter::{CallInputs, CreateInputs},
    primitives::{
        Account, AccountInfo, Address, Bytecode, Bytes, CreateScheme, EVMResult, Env, Eval,
        ExecutionResult as rExecutionResult, Halt as rHalt, HashMap as rHashMap, OutOfGasError,
        Output, ResultAndState, StorageSlot, B256, KECCAK_EMPTY, U256 as rU256,
    },
    Database, JournaledState,
};
use zksync_basic_types::{L2ChainId, H256};
use zksync_state::{ReadStorage, StoragePtr, WriteStorage};
use zksync_types::{
    ethabi, fee::Fee, l2::L2Tx, transaction_request::PaymasterParams, PackedEthSignature,
    StorageKey, Transaction, ACCOUNT_CODE_STORAGE_ADDRESS, CONTRACT_DEPLOYER_ADDRESS, U256,
};
use zksync_utils::{h256_to_account_address, h256_to_u256, u256_to_h256};

use crate::zk::{
    db::ZKVMData,
    env::{create_l1_batch_env, create_system_env},
};

use super::storage_view::StorageView;

const DEFAULT_CHAIN_ID: u32 = 31337;

pub(crate) fn create<'a, DB, E>(
    call: &CreateInputs,
    contract: &DualCompiledContract,
    env: &'a mut Env,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) -> EVMResult<ResultAndState>
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    let caller = env.tx.caller;
    let calldata =
        encode_create_params(&call.scheme, contract.zk_bytecode_hash, Default::default());
    let factory_deps = vec![contract.zk_deployed_bytecode.clone()];
    let nonce = ZKVMData::new(db, journaled_state).get_tx_nonce(caller);

    let tx = L2Tx::new(
        CONTRACT_DEPLOYER_ADDRESS,
        calldata,
        nonce,
        Fee {
            gas_limit: fix_l2_gas_limit(env.tx.gas_limit.into()),
            max_fee_per_gas: fix_l2_gas_price(revm_u256_to_u256(rU256::ZERO)),
            max_priority_fee_per_gas: revm_u256_to_u256(
                env.tx.gas_priority_fee.unwrap_or_default(),
            ),
            gas_per_pubdata_limit: U256::from(800),
        },
        address_to_h160(caller),
        revm_u256_to_u256(call.value),
        Some(factory_deps),
        PaymasterParams::default(),
    );
    inspect(tx, env, db, journaled_state)
}

pub(crate) fn call<'a, DB, E>(
    call: &CallInputs,
    contract: Option<&DualCompiledContract>,
    env: &'a mut Env,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) -> EVMResult<ResultAndState>
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?call, "call tx");
    let caller = env.tx.caller;
    let factory_deps = contract.map(|contract| vec![contract.zk_deployed_bytecode.clone()]);
    let nonce: zksync_types::Nonce = ZKVMData::new(db, journaled_state).get_tx_nonce(caller);
    let tx = L2Tx::new(
        address_to_h160(call.contract),
        call.input.to_vec(),
        nonce,
        Fee {
            gas_limit: fix_l2_gas_limit(env.tx.gas_limit.into()),
            max_fee_per_gas: fix_l2_gas_price(revm_u256_to_u256(env.tx.gas_price)),
            max_priority_fee_per_gas: revm_u256_to_u256(
                env.tx.gas_priority_fee.unwrap_or_default(),
            ),
            gas_per_pubdata_limit: U256::from(800),
        },
        address_to_h160(caller),
        revm_u256_to_u256(call.transfer.value),
        factory_deps,
        PaymasterParams::default(),
    );
    inspect(tx, env, db, journaled_state)
}

fn inspect<'a, DB, E>(
    mut tx: L2Tx,
    env: &'a mut Env,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) -> EVMResult<E>
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    let mut era_db = ZKVMData::new_with_system_contracts(db, journaled_state);
    let is_create = tx.execute.contract_address == zksync_types::CONTRACT_DEPLOYER_ADDRESS;
    tracing::trace!(caller = ?env.tx.caller, "executing transaction in zk vm");

    let chain_id_u32 = if env.cfg.chain_id <= u32::MAX as u64 {
        env.cfg.chain_id as u32
    } else {
        tracing::warn!(provided = ?env.cfg.chain_id, using = DEFAULT_CHAIN_ID, "using default chain id as provided chain_id does not fit into u32");
        DEFAULT_CHAIN_ID
    };

    if tx.common_data.signature.is_empty() {
        // FIXME: This is a hack to make sure that the signature is not empty.
        // Fails without a signature here: https://github.com/matter-labs/zksync-era/blob/73a1e8ff564025d06e02c2689da238ae47bb10c3/core/lib/types/src/transaction_request.rs#L381
        tx.common_data.signature = PackedEthSignature::default().serialize_packed().into();
    }

    let modified_storage_keys = era_db.override_keys.clone();
    let storage_ptr = StorageView::new(&mut era_db, modified_storage_keys).into_rc_ptr();
    let (tx_result, bytecodes, modified_storage) = inspect_inner(
        tx,
        storage_ptr,
        L2ChainId::from(chain_id_u32),
        u64::max(env.block.basefee.to::<u64>(), 1000),
    );

    let execution_result = match tx_result.result {
        ExecutionResult::Success { output, .. } => {
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
            let result = ethabi::decode(&[ethabi::ParamType::Bytes], &output)
                .ok()
                .and_then(|result| result.first().cloned())
                .and_then(|result| result.into_bytes())
                .unwrap_or_default();
            info!("zk vm decoded result {}", hex::encode(&result));

            let address = if result.len() == 32 {
                Some(h256_to_account_address(&H256::from_slice(&result)))
            } else {
                None
            };
            let output = if is_create {
                Output::Create(Bytes::from(result), address.map(h160_to_address))
            } else {
                Output::Call(Bytes::from(result))
            };

            rExecutionResult::Success {
                reason: Eval::Return,
                gas_used: tx_result.statistics.gas_used as u64,
                gas_refunded: tx_result.refunds.gas_refunded as u64,
                logs,
                output,
            }
        }
        ExecutionResult::Revert { output } => {
            let output = match output {
                VmRevertReason::General { data, .. } => data,
                VmRevertReason::Unknown { data, .. } => data,
                _ => Vec::new(),
            };

            rExecutionResult::Revert {
                gas_used: env.tx.gas_limit - tx_result.refunds.gas_refunded as u64,
                output: Bytes::from(output),
            }
        }
        ExecutionResult::Halt { reason } => {
            tracing::error!("tx execution halted: {}", reason);
            let mapped_reason = match reason {
                Halt::NotEnoughGasProvided => rHalt::OutOfGas(OutOfGasError::BasicOutOfGas),
                _ => rHalt::PrecompileError,
            };
            rExecutionResult::Halt {
                reason: mapped_reason,
                gas_used: env.tx.gas_limit - tx_result.refunds.gas_refunded as u64,
            }
        }
    };

    let mut state: rHashMap<Address, Account> = Default::default();
    let mut storage: rHashMap<Address, rHashMap<rU256, StorageSlot>> = Default::default();
    let mut codes: rHashMap<Address, (B256, Bytecode)> = Default::default();
    for (k, v) in &modified_storage {
        let address = h160_to_address(*k.address());
        let index = h256_to_revm_u256(*k.key());
        let previous = era_db.db.storage(address, index).unwrap_or_default();
        let entry = storage.entry(address).or_default();
        entry.insert(index, StorageSlot::new_changed(previous, h256_to_revm_u256(*v)));

        if k.address() == &ACCOUNT_CODE_STORAGE_ADDRESS {
            if let Some(bytecode) = bytecodes.get(&h256_to_u256(*v)) {
                let bytecode =
                    bytecode.iter().flat_map(|x| u256_to_h256(*x).to_fixed_bytes()).collect_vec();
                let bytecode = Bytecode::new_raw(Bytes::from(bytecode));
                let hash = B256::from_slice(v.as_bytes());
                codes.insert(h160_to_address(h256_to_h160(k.key())), (hash, bytecode));
            }
        }
    }

    for (address, storage) in storage {
        // println!("{:?} {:#?}", address, storage);
        let (info, status) = match era_db.db.basic(address).ok().flatten() {
            Some(info) => (info, revm::primitives::AccountStatus::Touched),
            None => (AccountInfo::default(), revm::primitives::AccountStatus::Created),
        };
        let account = Account {
            info: AccountInfo {
                balance: info.balance,
                nonce: info.nonce,
                code_hash: KECCAK_EMPTY,
                code: None,
            },
            storage,
            status,
        };
        state.insert(address, account);
    }

    for (address, (code_hash, code)) in codes {
        // println!("{:?} {:?}", address, code_hash);
        let (info, status) = match era_db.db.basic(address).ok().flatten() {
            Some(info) => (info, revm::primitives::AccountStatus::Touched),
            None => (AccountInfo::default(), revm::primitives::AccountStatus::Created),
        };
        let account = Account {
            info: AccountInfo {
                balance: info.balance,
                nonce: info.nonce,
                code_hash,
                code: Some(code),
            },
            storage: Default::default(),
            status,
        };
        state.insert(address, account);
    }

    for (address, new_account) in &state {
        let address = *address;
        let (account, _) =
            journaled_state.load_account(address, db).expect("account could not be loaded");
        let _ = std::mem::replace(&mut account.info.balance, new_account.info.balance);
        let _ = std::mem::replace(&mut account.info.nonce, new_account.info.nonce);
        account.info.code_hash = new_account.info.code_hash;
        account.info.code = new_account.info.code.clone();
        for (key, value) in &new_account.storage {
            let key = *key;
            journaled_state.sstore(address, key, value.present_value, db).expect("failed writing to slot");
        }
    }

    Ok(ResultAndState { result: execution_result, state })
}

fn inspect_inner<S: ReadStorage>(
    l2_tx: L2Tx,
    storage: StoragePtr<StorageView<S>>,
    chain_id: L2ChainId,
    l1_gas_price: u64,
) -> (VmExecutionResultAndLogs, HashMap<U256, Vec<U256>>, HashMap<StorageKey, H256>) {
    let batch_env = create_l1_batch_env(storage.clone(), l1_gas_price);

    let system_contracts = SystemContracts::from_options(&Options::BuiltInWithoutSecurity);
    let system_env = create_system_env(system_contracts.baseline_contracts, chain_id);

    let mut vm: Vm<_, HistoryDisabled> = Vm::new(batch_env.clone(), system_env, storage.clone());

    let tx: Transaction = l2_tx.clone().into();

    vm.push_transaction(tx.clone());
    let call_tracer_result = Arc::new(OnceCell::default());
    let tracers = vec![CallTracer::new(call_tracer_result.clone()).into_tracer_pointer()];
    let tx_result = vm.inspect(tracers.into(), VmExecutionMode::OneTx);
    let call_traces = Arc::try_unwrap(call_tracer_result).unwrap().take().unwrap_or_default();
    trace!(?tx_result.result, "zk vm result");

    match &tx_result.result {
        ExecutionResult::Success { output } => {
            let output = zksync_basic_types::Bytes::from(output.clone());
            tracing::debug!(?output, "Call: Successful");
        }
        ExecutionResult::Revert { output } => {
            tracing::debug!(?output, "Call: Reverted");
        }
        ExecutionResult::Halt { reason } => {
            tracing::debug!(?reason, "Call: Halted");
        }
    };
    formatter::print_vm_details(&tx_result);

    tracing::info!("=== Console Logs: ");
    let console_log_handler = ConsoleLogHandler::default();
    for call in &call_traces {
        console_log_handler.handle_call_recursive(call);
    }

    let resolve_hashes = get_env_var::<bool>("ZK_DEBUG_RESOLVE_HASHES");
    tracing::info!("=== Calls: ");
    for call in call_traces.iter() {
        formatter::print_call(call, 0, &ShowCalls::All, resolve_hashes);
    }

    tracing::info!("==== {}", format!("{} events", tx_result.logs.events.len()));
    for event in &tx_result.logs.events {
        formatter::print_event(event, resolve_hashes);
    }

    let bytecodes = vm
        .get_last_tx_compressed_bytecodes()
        .iter()
        .map(|b| bytecode_to_factory_dep(b.original.clone()))
        .collect();
    let modified_keys = storage.borrow().modified_storage_keys().clone();
    (tx_result, bytecodes, modified_keys)
}

/// Prepares calldata to invoke deployer contract.
fn encode_create_params(
    scheme: &CreateScheme,
    contract_hash: H256,
    constructor_input: Vec<u8>,
) -> Vec<u8> {
    let (name, salt) = match scheme {
        CreateScheme::Create => ("create", H256::zero()),
        CreateScheme::Create2 { salt } => ("create2", u256_to_h256(revm_u256_to_u256(*salt))),
    };

    // TODO (SMA-1608): We should not re-implement the ABI parts in different places, instead have
    // the ABI available  from the `zksync_contracts` crate.
    let signature = ethabi::short_signature(
        name,
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

fn get_env_var<T>(name: &str) -> T
where
    T: FromStr + Default,
    T::Err: Debug,
{
    std::env::var(name)
        .map(|value| {
            value.parse::<T>().unwrap_or_else(|err| {
                panic!("failed parsing env variable {}={}, {:?}", name, value, err)
            })
        })
        .unwrap_or_default()
}
