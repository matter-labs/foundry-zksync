use alloy_primitives::hex;
use foundry_zksync_compiler::DualCompiledContract;
use itertools::Itertools;
use revm::{
    interpreter::{CallInputs, CallScheme, CallValue, CreateInputs},
    primitives::{Address, CreateScheme, Env, ResultAndState, TransactTo, B256, U256 as rU256},
    Database, EvmContext, InnerEvmContext,
};
use tracing::{debug, error, info};
use zksync_basic_types::H256;
use zksync_types::{
    ethabi, fee::Fee, l2::L2Tx, transaction_request::PaymasterParams, CONTRACT_DEPLOYER_ADDRESS,
    U256,
};

use std::{cmp::min, collections::HashMap, fmt::Debug};

use crate::{
    convert::{ConvertAddress, ConvertH160, ConvertRU256, ConvertU256},
    fix_l2_gas_limit, fix_l2_gas_price,
    vm::{
        db::ZKVMData,
        inspect::{inspect, inspect_as_batch, ZKVMExecutionResult, ZKVMResult},
        tracers::cheatcode::{CallContext, CheatcodeTracerContext},
    },
};

/// Transacts
pub fn transact<'a, DB>(
    persisted_factory_deps: Option<&'a mut HashMap<H256, Vec<u8>>>,
    factory_deps: Option<Vec<Vec<u8>>>,
    env: &'a mut Env,
    db: &'a mut DB,
) -> eyre::Result<ResultAndState>
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(calldata = ?env.tx.data, fdeps = factory_deps.as_ref().map(|deps| deps.iter().map(|dep| dep.len()).join(",")).unwrap_or_default(), "zk transact");

    let mut ecx = EvmContext::new_with_env(db, Box::new(env.clone()));
    let caller = env.tx.caller;
    let nonce = ZKVMData::new(&mut ecx).get_tx_nonce(caller);
    let (transact_to, is_create) = match env.tx.transact_to {
        TransactTo::Call(to) => (to.to_h160(), false),
        TransactTo::Create => (CONTRACT_DEPLOYER_ADDRESS, true),
    };

    let (gas_limit, max_fee_per_gas) = gas_params(&mut ecx, caller, &PaymasterParams::default());
    debug!(?gas_limit, ?max_fee_per_gas, "tx gas parameters");
    let tx = L2Tx::new(
        Some(transact_to),
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
        factory_deps.unwrap_or_default(),
        PaymasterParams::default(),
    );

    let call_ctx = CallContext {
        tx_caller: env.tx.caller,
        msg_sender: env.tx.caller,
        contract: transact_to.to_address(),
        delegate_as: None,
        block_number: env.block.number,
        block_timestamp: env.block.timestamp,
        block_hashes: get_historical_block_hashes(&mut ecx),
        block_basefee: min(max_fee_per_gas.to_ru256(), env.block.basefee),
        is_create,
        is_static: false,
    };

    let mut ccx = CheatcodeTracerContext { persisted_factory_deps, ..Default::default() };

    match inspect::<_, DB::Error>(tx, &mut ecx, &mut ccx, call_ctx) {
        Ok(ZKVMExecutionResult { execution_result: result, .. }) => {
            Ok(ResultAndState { result, state: ecx.journaled_state.finalize().0 })
        }
        Err(err) => eyre::bail!("zk backend: failed while inspecting: {err:?}"),
    }
}

/// Retrieves L2 ETH balance for a given address.
pub fn balance<DB>(address: Address, ecx: &mut EvmContext<DB>) -> rU256
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    let balance = ZKVMData::new(ecx).get_balance(address);
    balance.to_ru256()
}

/// Retrieves bytecode hash stored at a given address.
#[allow(dead_code)]
pub fn code_hash<DB>(address: Address, ecx: &mut EvmContext<DB>) -> B256
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    B256::from(ZKVMData::new(ecx).get_code_hash(address).0)
}

/// Retrieves nonce for a given address.
pub fn nonce<DB>(address: Address, ecx: &mut InnerEvmContext<DB>) -> u32
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    ZKVMData::new(ecx).get_tx_nonce(address).0
}

/// Executes a CREATE opcode on the ZK-VM.
pub fn create<DB, E>(
    call: &CreateInputs,
    contract: &DualCompiledContract,
    factory_deps: Vec<Vec<u8>>,
    ecx: &mut EvmContext<DB>,
    mut ccx: CheatcodeTracerContext,
) -> ZKVMResult<E>
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?call, "create tx {}", hex::encode(&call.init_code));
    let constructor_input = call.init_code[contract.evm_bytecode.len()..].to_vec();
    let caller = ecx.env.tx.caller;
    let calldata = encode_create_params(&call.scheme, contract.zk_bytecode_hash, constructor_input);
    let nonce = ZKVMData::new(ecx).get_tx_nonce(caller);

    let paymaster_params = if let Some(paymaster_data) = &ccx.paymaster_data {
        PaymasterParams {
            paymaster: paymaster_data.address.to_h160(),
            paymaster_input: paymaster_data.input.to_vec(),
        }
    } else {
        PaymasterParams::default()
    };

    let (gas_limit, max_fee_per_gas) = gas_params(ecx, caller, &paymaster_params);
    info!(?gas_limit, ?max_fee_per_gas, "tx gas parameters");

    let tx = L2Tx::new(
        Some(CONTRACT_DEPLOYER_ADDRESS),
        calldata,
        nonce,
        Fee {
            gas_limit,
            max_fee_per_gas,
            max_priority_fee_per_gas: ecx.env.tx.gas_priority_fee.unwrap_or_default().to_u256(),
            gas_per_pubdata_limit: U256::from(20000),
        },
        caller.to_h160(),
        call.value.to_u256(),
        factory_deps,
        paymaster_params,
    );

    let call_ctx = CallContext {
        tx_caller: ecx.env.tx.caller,
        msg_sender: call.caller,
        contract: CONTRACT_DEPLOYER_ADDRESS.to_address(),
        delegate_as: None,
        block_number: ecx.env.block.number,
        block_timestamp: ecx.env.block.timestamp,
        block_basefee: min(max_fee_per_gas.to_ru256(), ecx.env.block.basefee),
        block_hashes: get_historical_block_hashes(ecx),
        is_create: true,
        is_static: false,
    };

    inspect_as_batch(tx, ecx, &mut ccx, call_ctx)
}

/// Executes a CALL opcode on the ZK-VM.
pub fn call<DB, E>(
    call: &CallInputs,
    factory_deps: Vec<Vec<u8>>,
    ecx: &mut EvmContext<DB>,
    mut ccx: CheatcodeTracerContext,
) -> ZKVMResult<E>
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    info!(?call, "call tx {}", hex::encode(&call.input));
    let caller = ecx.env.tx.caller;
    let nonce: zksync_types::Nonce = ZKVMData::new(ecx).get_tx_nonce(caller);

    let paymaster_params = if let Some(paymaster_data) = &ccx.paymaster_data {
        PaymasterParams {
            paymaster: paymaster_data.address.to_h160(),
            paymaster_input: paymaster_data.input.to_vec(),
        }
    } else {
        PaymasterParams::default()
    };

    let (gas_limit, max_fee_per_gas) = gas_params(ecx, caller, &paymaster_params);
    info!(?gas_limit, ?max_fee_per_gas, "tx gas parameters");

    let tx = L2Tx::new(
        Some(call.bytecode_address.to_h160()),
        call.input.to_vec(),
        nonce,
        Fee {
            gas_limit,
            max_fee_per_gas,
            max_priority_fee_per_gas: ecx.env.tx.gas_priority_fee.unwrap_or_default().to_u256(),
            gas_per_pubdata_limit: U256::from(20000),
        },
        caller.to_h160(),
        match call.value {
            CallValue::Transfer(value) => value.to_u256(),
            _ => U256::zero(),
        },
        factory_deps,
        paymaster_params,
    );

    // address and caller are specific to the type of call:
    // Call | StaticCall => { address: to, caller: contract.address }
    // CallCode          => { address: contract.address, caller: contract.address }
    // DelegateCall      => { address: contract.address, caller: contract.caller }
    let call_ctx = CallContext {
        tx_caller: ecx.env.tx.caller,
        msg_sender: call.caller,
        contract: call.bytecode_address,
        delegate_as: match call.scheme {
            CallScheme::DelegateCall => Some(call.target_address),
            _ => None,
        },
        block_number: ecx.env.block.number,
        block_timestamp: ecx.env.block.timestamp,
        block_hashes: get_historical_block_hashes(ecx),
        block_basefee: min(max_fee_per_gas.to_ru256(), ecx.env.block.basefee),
        is_create: false,
        is_static: call.is_static,
    };

    inspect(tx, ecx, &mut ccx, call_ctx)
}

/// Assign gas parameters that satisfy zkSync's fee model.
fn gas_params<DB>(
    ecx: &mut EvmContext<DB>,
    caller: Address,
    paymaster_params: &PaymasterParams,
) -> (U256, U256)
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    let value = ecx.env.tx.value.to_u256();
    let balance = ZKVMData::new(ecx).get_balance(caller);
    if balance.is_zero() {
        error!("balance is 0 for {caller:?}, transaction will fail");
    }
    let max_fee_per_gas = fix_l2_gas_price(ecx.env.tx.gas_price.to_u256());

    let use_paymaster = !paymaster_params.paymaster.is_zero();

    // We check if the paymaster is set, if it is not set, we use the proposed gas limit
    let gas_limit = if use_paymaster {
        ecx.env.tx.gas_limit.into()
    } else {
        fix_l2_gas_limit(ecx.env.tx.gas_limit.into(), max_fee_per_gas, value, balance)
    };

    (gas_limit, max_fee_per_gas)
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

/// Get historical block hashes mapped to block numbers. This excludes the current block.
fn get_historical_block_hashes<DB: Database>(ecx: &mut EvmContext<DB>) -> HashMap<rU256, B256> {
    let mut block_hashes = HashMap::default();
    let num_blocks = get_env_historical_block_count();
    tracing::debug!("fetching last {num_blocks} block hashes");
    for i in 1..=num_blocks {
        let (block_number, overflow) =
            ecx.env.block.number.overflowing_sub(alloy_primitives::U256::from(i));
        if overflow {
            break
        }
        match ecx.block_hash(block_number.to_u256().as_u64()) {
            Ok(block_hash) => {
                block_hashes.insert(block_number, block_hash);
            }
            Err(_) => break,
        }
    }

    block_hashes
}

/// Get the number of historical blocks to fetch, from the env.
/// Default: `256`.
fn get_env_historical_block_count() -> u32 {
    let name = "ZK_DEBUG_HISTORICAL_BLOCK_HASHES";
    std::env::var(name)
        .map(|value| {
            value.parse::<u32>().unwrap_or_else(|err| {
                panic!("failed parsing env variable {}={}, {:?}", name, value, err)
            })
        })
        .map(|num| num.min(256))
        .unwrap_or(256)
}
