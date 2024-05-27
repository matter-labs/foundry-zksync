use crate::{
    convert::{ConvertAddress, ConvertH160, ConvertH256, ConvertRU256, ConvertU256},
    is_system_address,
    vm::tracer::{CallContext, CheatcodeTracer},
};
use alloy_primitives::Log;
use alloy_sol_types::{SolEvent, SolInterface, SolValue};
use ansi_term::Color::Cyan;
use era_test_node::{
    formatter,
    node::ShowCalls,
    system_contracts::{Options, SystemContracts},
    utils::bytecode_to_factory_dep,
};
use foundry_common::{
    console::HARDHAT_CONSOLE_ADDRESS, fmt::ConsoleFmt, patch_hh_console_selector, Console,
    HardhatConsole,
};
use foundry_zksync_compiler::DualCompiledContract;
use std::{cmp::min, collections::HashMap, fmt::Debug, str::FromStr, sync::Arc};

use crate::{fix_l2_gas_limit, fix_l2_gas_price};
use itertools::Itertools;
use multivm::{
    interface::{Halt, VmExecutionResultAndLogs, VmInterface, VmRevertReason},
    tracers::CallTracer,
    vm_latest::{ExecutionResult, HistoryDisabled, ToTracerPointer, Vm, VmExecutionMode},
};
use once_cell::sync::OnceCell;
use revm::{
    interpreter::{CallInputs, CallScheme, CreateInputs},
    precompile::Precompiles,
    primitives::{
        Address, Bytecode, Bytes, CreateScheme, EVMResultGeneric, Env, Eval,
        ExecutionResult as rExecutionResult, Halt as rHalt, HashMap as rHashMap, OutOfGasError,
        Output, ResultAndState, SpecId, StorageSlot, TransactTo, B256, U256 as rU256,
    },
    Database, JournaledState,
};
use tracing::{info, trace};
use zksync_basic_types::{L2ChainId, H256};
use zksync_state::{ReadStorage, StoragePtr, WriteStorage};
use zksync_types::{
    ethabi, fee::Fee, l2::L2Tx, transaction_request::PaymasterParams, vm_trace::Call,
    PackedEthSignature, StorageKey, Transaction, VmEvent, ACCOUNT_CODE_STORAGE_ADDRESS,
    CONTRACT_DEPLOYER_ADDRESS, H160, U256,
};
use zksync_utils::{h256_to_account_address, h256_to_u256, u256_to_h256};

use crate::vm::{
    db::{ZKVMData, DEFAULT_CHAIN_ID},
    env::{create_l1_batch_env, create_system_env},
};

use super::{storage_view::StorageView, tracer::CheatcodeTracerContext};

/// Maximum gas price allowed for L1.
const MAX_L1_GAS_PRICE: u64 = 1000;

type ZKVMResult<E> = EVMResultGeneric<rExecutionResult, E>;

/// Transacts
pub fn transact<'a, DB>(
    factory_deps: Option<Vec<Vec<u8>>>,
    env: &'a mut Env,
    db: &'a mut DB,
) -> eyre::Result<ResultAndState>
where
    DB: Database + Send,
    <DB as Database>::Error: Debug,
{
    tracing::debug!("zk transact");
    let mut journaled_state = JournaledState::new(
        env.cfg.spec_id,
        Precompiles::new(to_precompile_id(env.cfg.spec_id))
            .addresses()
            .into_iter()
            .copied()
            .collect(),
    );

    let caller = env.tx.caller;
    let nonce = ZKVMData::new(db, &mut journaled_state).get_tx_nonce(caller);
    let (transact_to, is_create) = match env.tx.transact_to {
        TransactTo::Call(to) => (to.to_h160(), false),
        TransactTo::Create(CreateScheme::Create) |
        TransactTo::Create(CreateScheme::Create2 { .. }) => (CONTRACT_DEPLOYER_ADDRESS, true),
    };

    let (gas_limit, max_fee_per_gas) = gas_params(env, db, &mut journaled_state, caller);
    let tx = L2Tx::new(
        transact_to,
        env.tx.data.to_vec(),
        nonce,
        Fee {
            gas_limit,
            max_fee_per_gas,
            max_priority_fee_per_gas: env.tx.gas_priority_fee.unwrap_or_default().to_u256(),
            gas_per_pubdata_limit: U256::from(20000),
        },
        caller.to_h160(),
        env.tx.value.to_u256(),
        factory_deps,
        PaymasterParams::default(),
    );

    let call_ctx = CallContext {
        tx_caller: env.tx.caller,
        msg_sender: env.tx.caller,
        contract: transact_to.to_address(),
        delegate_as: None,
        block_number: env.block.number,
        block_timestamp: env.block.timestamp,
        block_basefee: min(max_fee_per_gas.to_ru256(), env.block.basefee),
        is_create,
    };

    match inspect::<_, DB::Error>(tx, env, db, &mut journaled_state, Default::default(), call_ctx) {
        Ok(result) => Ok(ResultAndState { result, state: journaled_state.finalize().0 }),
        Err(err) => eyre::bail!("zk backend: failed while inspecting: {err:?}"),
    }
}

/// Retrieves L2 ETH balance for a given address.
pub fn balance<'a, DB>(
    address: Address,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) -> rU256
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    let balance = ZKVMData::new(db, journaled_state).get_balance(address);
    balance.to_ru256()
}

/// Retrieves bytecode hash stored at a given address.
#[allow(dead_code)]
pub fn code_hash<'a, DB>(
    address: Address,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) -> B256
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    B256::from(ZKVMData::new(db, journaled_state).get_code_hash(address).0)
}

/// Retrieves nonce for a given address.
pub fn nonce<'a, DB>(
    address: Address,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
) -> u32
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    ZKVMData::new(db, journaled_state).get_tx_nonce(address).0
}

/// Executes a CREATE opcode on the ZK-VM.
pub fn create<'a, DB, E>(
    call: &CreateInputs,
    contract: &DualCompiledContract,
    factory_deps: Vec<Vec<u8>>,
    env: &'a mut Env,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
    ccx: CheatcodeTracerContext,
) -> ZKVMResult<E>
where
    DB: Database + Send,
    <DB as Database>::Error: Debug,
{
    info!(?call, "create tx {}", hex::encode(&call.init_code));
    let constructor_input = call.init_code[contract.evm_bytecode.len()..].to_vec();
    let caller = env.tx.caller;
    let calldata = encode_create_params(&call.scheme, contract.zk_bytecode_hash, constructor_input);
    let nonce = ZKVMData::new(db, journaled_state).get_tx_nonce(caller);

    let (gas_limit, max_fee_per_gas) = gas_params(env, db, journaled_state, caller);
    let tx = L2Tx::new(
        CONTRACT_DEPLOYER_ADDRESS,
        calldata,
        nonce,
        Fee {
            gas_limit,
            max_fee_per_gas,
            max_priority_fee_per_gas: env.tx.gas_priority_fee.unwrap_or_default().to_u256(),
            gas_per_pubdata_limit: U256::from(20000),
        },
        caller.to_h160(),
        call.value.to_u256(),
        Some(factory_deps),
        PaymasterParams::default(),
    );

    let call_ctx = CallContext {
        tx_caller: env.tx.caller,
        msg_sender: call.caller,
        contract: CONTRACT_DEPLOYER_ADDRESS.to_address(),
        delegate_as: None,
        block_number: env.block.number,
        block_timestamp: env.block.timestamp,
        block_basefee: min(max_fee_per_gas.to_ru256(), env.block.basefee),
        is_create: true,
    };

    inspect(tx, env, db, journaled_state, ccx, call_ctx)
}

/// Executes a CALL opcode on the ZK-VM.
pub fn call<'a, DB, E>(
    call: &CallInputs,
    env: &'a mut Env,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
    ccx: CheatcodeTracerContext,
) -> ZKVMResult<E>
where
    DB: Database + Send,
    <DB as Database>::Error: Debug,
{
    info!(?call, "call tx {}", hex::encode(&call.input));
    let caller = env.tx.caller;
    let nonce: zksync_types::Nonce = ZKVMData::new(db, journaled_state).get_tx_nonce(caller);

    let (gas_limit, max_fee_per_gas) = gas_params(env, db, journaled_state, caller);
    let tx = L2Tx::new(
        call.contract.to_h160(),
        call.input.to_vec(),
        nonce,
        Fee {
            gas_limit,
            max_fee_per_gas,
            max_priority_fee_per_gas: env.tx.gas_priority_fee.unwrap_or_default().to_u256(),
            gas_per_pubdata_limit: U256::from(20000),
        },
        caller.to_h160(),
        call.transfer.value.to_u256(),
        None,
        PaymasterParams::default(),
    );

    // address and caller are specific to the type of call:
    // Call | StaticCall => { address: to, caller: contract.address }
    // CallCode          => { address: contract.address, caller: contract.address }
    // DelegateCall      => { address: contract.address, caller: contract.caller }
    let call_ctx = CallContext {
        tx_caller: env.tx.caller,
        msg_sender: call.context.caller,
        contract: call.contract,
        delegate_as: match call.context.scheme {
            CallScheme::DelegateCall => Some(call.context.address),
            _ => None,
        },
        block_number: env.block.number,
        block_timestamp: env.block.timestamp,
        block_basefee: min(max_fee_per_gas.to_ru256(), env.block.basefee),
        is_create: false,
    };

    inspect(tx, env, db, journaled_state, ccx, call_ctx)
}

/// Assign gas parameters that satisfy zkSync's fee model.
fn gas_params<'a, DB>(
    env: &'a mut Env,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
    caller: Address,
) -> (U256, U256)
where
    DB: Database + Send,
    <DB as Database>::Error: Debug,
{
    let value = env.tx.value.to_u256();
    let balance = ZKVMData::new(db, journaled_state).get_balance(caller);
    if balance.is_zero() {
        tracing::error!("balance is 0 for {caller:?}, transaction will fail");
    }
    let max_fee_per_gas = fix_l2_gas_price(env.tx.gas_price.to_u256());
    let gas_limit = fix_l2_gas_limit(env.tx.gas_limit.into(), max_fee_per_gas, value, balance);

    (gas_limit, max_fee_per_gas)
}

fn inspect<'a, DB, E>(
    mut tx: L2Tx,
    env: &'a mut Env,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
    mut ccx: CheatcodeTracerContext,
    call_ctx: CallContext,
) -> ZKVMResult<E>
where
    DB: Database + Send,
    <DB as Database>::Error: Debug,
{
    let mut era_db = ZKVMData::new_with_system_contracts(db, journaled_state)
        .with_extra_factory_deps(std::mem::take(&mut ccx.persisted_factory_deps))
        .with_storage_accesses(ccx.accesses.take());

    let is_create = call_ctx.is_create;
    tracing::info!(?call_ctx, "executing transaction in zk vm");

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
    let storage_ptr =
        StorageView::new(&mut era_db, modified_storage_keys, tx.common_data.initiator_address)
            .into_rc_ptr();
    let (tx_result, bytecodes, modified_storage) =
        inspect_inner(tx, storage_ptr, L2ChainId::from(chain_id_u32), ccx, call_ctx);

    if let Some(record) = &mut era_db.accesses {
        for k in modified_storage.keys() {
            record.writes.entry(k.address().to_address()).or_default().push(k.key().to_ru256());
        }
    }

    let execution_result = match tx_result.result {
        ExecutionResult::Success { output, .. } => {
            let logs = tx_result
                .logs
                .events
                .clone()
                .into_iter()
                .map(|event| revm::primitives::Log {
                    address: event.address.to_address(),
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
                Output::Create(Bytes::from(result), address.map(ConvertH160::to_address))
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

    let mut storage: rHashMap<Address, rHashMap<rU256, StorageSlot>> = Default::default();
    let mut codes: rHashMap<Address, (B256, Bytecode)> = Default::default();
    for (k, v) in &modified_storage {
        let address = k.address().to_address();
        let index = k.key().to_ru256();
        era_db.load_account(address);
        let previous = era_db.sload(address, index);
        let entry = storage.entry(address).or_default();
        entry.insert(index, StorageSlot::new_changed(previous, v.to_ru256()));

        if k.address() == &ACCOUNT_CODE_STORAGE_ADDRESS {
            if let Some(bytecode) = bytecodes.get(&h256_to_u256(*v)) {
                let bytecode =
                    bytecode.iter().flat_map(|x| u256_to_h256(*x).to_fixed_bytes()).collect_vec();
                let bytecode = Bytecode::new_raw(Bytes::from(bytecode));
                let hash = B256::from_slice(v.as_bytes());
                codes.insert(k.key().to_h160().to_address(), (hash, bytecode));
            } else {
                // We populate bytecodes for all non-system addresses
                if !is_system_address(k.key().to_h160().to_address()) {
                    if let Some(bytecode) = (&mut era_db).load_factory_dep(*v) {
                        let hash = B256::from_slice(v.as_bytes());
                        let bytecode = Bytecode::new_raw(Bytes::from(bytecode));
                        codes.insert(k.key().to_h160().to_address(), (hash, bytecode));
                    } else {
                        tracing::warn!(
                            "no bytecode was found for {:?}, requested by account {:?}",
                            *v,
                            k.key().to_h160()
                        );
                    }
                }
            }
        }
    }

    for (address, storage) in storage {
        journaled_state.load_account(address, db).expect("account could not be loaded");
        journaled_state.touch(&address);

        for (key, value) in storage {
            journaled_state
                .sstore(address, key, value.present_value, db)
                .expect("failed writing to slot");
        }
    }

    for (address, (code_hash, code)) in codes {
        journaled_state.load_account(address, db).expect("account could not be loaded");
        journaled_state.touch(&address);
        let account = journaled_state.state.get_mut(&address).expect("account is loaded");

        account.info.code_hash = code_hash;
        account.info.code = Some(code);
    }

    Ok(execution_result)
}

fn inspect_inner<S: ReadStorage + Send>(
    l2_tx: L2Tx,
    storage: StoragePtr<StorageView<S>>,
    chain_id: L2ChainId,
    mut ccx: CheatcodeTracerContext,
    call_ctx: CallContext,
) -> (VmExecutionResultAndLogs, HashMap<U256, Vec<U256>>, HashMap<StorageKey, H256>) {
    let l1_gas_price = call_ctx.block_basefee.to::<u64>().max(MAX_L1_GAS_PRICE);
    let fair_l2_gas_price = call_ctx.block_basefee.saturating_to::<u64>();
    let batch_env = create_l1_batch_env(storage.clone(), l1_gas_price, fair_l2_gas_price);

    let system_contracts = SystemContracts::from_options(&Options::BuiltInWithoutSecurity);
    let system_env = create_system_env(system_contracts.baseline_contracts, chain_id);

    let mut vm: Vm<_, HistoryDisabled> = Vm::new(batch_env.clone(), system_env, storage.clone());

    let tx: Transaction = l2_tx.clone().into();

    vm.push_transaction(tx.clone());
    let call_tracer_result = Arc::new(OnceCell::default());
    let cheatcode_tracer_result = Arc::new(OnceCell::default());
    let mut expected_calls = HashMap::<_, _>::new();
    if let Some(ec) = &ccx.expected_calls {
        for (addr, v) in ec.iter() {
            expected_calls.insert(*addr, v.clone());
        }
    }
    let tracers = vec![
        CallTracer::new(call_tracer_result.clone()).into_tracer_pointer(),
        CheatcodeTracer::new(
            ccx.mocked_calls,
            expected_calls,
            cheatcode_tracer_result.clone(),
            call_ctx,
        )
        .into_tracer_pointer(),
    ];
    let mut tx_result = vm.inspect(tracers.into(), VmExecutionMode::OneTx);
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

    // update expected calls from cheatcode tracer's result
    let cheatcode_result =
        Arc::try_unwrap(cheatcode_tracer_result).unwrap().take().unwrap_or_default();
    if let Some(expected_calls) = ccx.expected_calls.as_mut() {
        expected_calls.extend(cheatcode_result.expected_calls);
    }

    formatter::print_vm_details(&tx_result);

    tracing::info!("=== Console Logs: ");
    let log_parser = ConsoleLogParser::new();
    let console_logs = log_parser.get_logs(&call_traces, true);

    for log in console_logs {
        tx_result.logs.events.push(VmEvent {
            location: Default::default(),
            address: H160::zero(),
            indexed_topics: log.topics().iter().map(|topic| H256::from(topic.0)).collect(),
            value: log.data.data.to_vec(),
        });
    }

    let resolve_hashes = get_env_var::<bool>("ZK_DEBUG_RESOLVE_HASHES");
    let show_outputs = get_env_var::<bool>("ZK_DEBUG_SHOW_OUTPUTS");
    tracing::info!("=== Calls: ");
    for call in call_traces.iter() {
        formatter::print_call(call, 0, &ShowCalls::All, show_outputs, resolve_hashes);
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

struct ConsoleLogParser {
    hardhat_console_address: H160,
}

impl ConsoleLogParser {
    fn new() -> Self {
        Self { hardhat_console_address: HARDHAT_CONSOLE_ADDRESS.to_h160() }
    }

    pub fn get_logs(&self, call_traces: &[Call], print: bool) -> Vec<Log> {
        let mut logs = vec![];
        for call in call_traces {
            self.parse_call_recursive(call, &mut logs, print);
        }
        logs
    }

    fn parse_call_recursive(&self, current_call: &Call, logs: &mut Vec<Log>, print: bool) {
        self.parse_call(current_call, logs, print);
        for call in &current_call.calls {
            self.parse_call_recursive(call, logs, print);
        }
    }

    fn parse_call(&self, current_call: &Call, logs: &mut Vec<Log>, print: bool) {
        if current_call.to != self.hardhat_console_address {
            return;
        }
        if current_call.input.len() < 4 {
            return;
        }

        let mut input = current_call.input.clone();

        // Patch the Hardhat-style selector (`uint` instead of `uint256`)
        patch_hh_console_selector(&mut input);

        // Decode the call
        let Ok(call) = HardhatConsole::HardhatConsoleCalls::abi_decode(&input, false) else {
            return;
        };

        // Convert the parameters of the call to their string representation using `ConsoleFmt`.
        let message = call.fmt(Default::default());
        let log = Log::new(
            Address::default(),
            vec![Console::log::SIGNATURE_HASH],
            message.abi_encode().into(),
        )
        .unwrap_or_else(|| Log { ..Default::default() });

        logs.push(log);

        if print {
            tracing::info!("{}", Cyan.paint(message));
        }
    }
}

/// Prepares calldata to invoke deployer contract.
pub fn encode_create_params(
    scheme: &CreateScheme,
    contract_hash: H256,
    constructor_input: Vec<u8>,
) -> Vec<u8> {
    let (name, salt) = match scheme {
        CreateScheme::Create => ("create", H256::zero()),
        CreateScheme::Create2 { salt } => ("create2", salt.to_h256()),
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

fn to_precompile_id(spec_id: SpecId) -> revm::precompile::SpecId {
    match spec_id {
        SpecId::FRONTIER |
        SpecId::FRONTIER_THAWING |
        SpecId::HOMESTEAD |
        SpecId::DAO_FORK |
        SpecId::TANGERINE |
        SpecId::SPURIOUS_DRAGON => revm::precompile::SpecId::HOMESTEAD,
        SpecId::BYZANTIUM | SpecId::CONSTANTINOPLE | SpecId::PETERSBURG => {
            revm::precompile::SpecId::BYZANTIUM
        }
        SpecId::ISTANBUL | SpecId::MUIR_GLACIER => revm::precompile::SpecId::ISTANBUL,
        SpecId::BERLIN |
        SpecId::LONDON |
        SpecId::ARROW_GLACIER |
        SpecId::GRAY_GLACIER |
        SpecId::MERGE |
        SpecId::SHANGHAI |
        SpecId::CANCUN |
        SpecId::BEDROCK |
        SpecId::REGOLITH |
        SpecId::CANYON |
        SpecId::LATEST => revm::precompile::SpecId::BERLIN,
    }
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
