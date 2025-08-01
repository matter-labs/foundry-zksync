use alloy_evm::{EvmEnv, eth::EthEvmContext};
use alloy_primitives::{hex, map::HashMap};
use itertools::Itertools;
use revm::{
    Database, Journal,
    context::{CreateScheme, JournalTr, TransactTo, TxEnv},
    context_interface::result::ResultAndState,
    interpreter::{CallInputs, CallScheme, CallValue},
    primitives::{Address, B256, U256 as rU256},
};
use tracing::{debug, info};
use zksync_basic_types::H256;
use zksync_types::{
    CONTRACT_DEPLOYER_ADDRESS, CREATE2_FACTORY_ADDRESS, U256, ethabi, fee::Fee, l2::L2Tx,
    transaction_request::PaymasterParams,
};
use zksync_vm_interface::Call;

use core::convert::Into;
use std::{cmp::min, fmt::Debug};

use crate::{
    convert::{ConvertAddress, ConvertH160, ConvertRU256, ConvertU256},
    vm::{
        db::ZKVMData,
        inspect::{ZKVMExecutionResult, ZKVMResult, gas_params, inspect, inspect_as_batch},
        tracers::cheatcode::{CallContext, CheatcodeTracerContext},
    },
};

use super::ZkEnv;

/// Transacts
pub fn transact<'a, DB>(
    persisted_factory_deps: Option<&'a mut HashMap<H256, Vec<u8>>>,
    factory_deps: Option<Vec<Vec<u8>>>,
    paymaster_data: Option<PaymasterParams>,
    evm_env: EvmEnv,
    tx: TxEnv,
    zk_env: &ZkEnv,
    db: &'a mut DB,
) -> eyre::Result<(ResultAndState, Vec<Call>)>
where
    DB: Database + ?Sized,
    <DB as Database>::Error: Debug,
{
    info!(calldata = ?tx.data, fdeps = factory_deps.as_ref().map(|deps| deps.iter().map(|dep| dep.len()).join(",")).unwrap_or_default(), "zk transact");

    let paymaster_params = PaymasterParams {
        paymaster: paymaster_data.as_ref().map_or_else(Default::default, |data| data.paymaster),
        paymaster_input: paymaster_data
            .as_ref()
            .map_or_else(Vec::new, |data| data.paymaster_input.to_vec()),
    };

    let mut ecx = EthEvmContext {
        block: evm_env.block_env,
        cfg: evm_env.cfg_env,
        tx,
        journaled_state: Journal::new(db),
        local: Default::default(),
        chain: (),
        error: Ok(()),
    };
    let caller = ecx.tx.caller;
    let nonce = ZKVMData::new(&mut ecx).get_tx_nonce(caller);
    let (transact_to, is_create) = match ecx.tx.kind {
        TransactTo::Call(to) => {
            let to = to.to_h160();
            (to, to == CONTRACT_DEPLOYER_ADDRESS || to == CREATE2_FACTORY_ADDRESS)
        }
        TransactTo::Create => (CONTRACT_DEPLOYER_ADDRESS, true),
    };

    let (gas_limit, max_fee_per_gas) = gas_params(&mut ecx, caller, &paymaster_params);
    debug!(?gas_limit, ?max_fee_per_gas, "tx gas parameters");
    let tx = L2Tx::new(
        Some(transact_to),
        ecx.tx.data.to_vec(),
        (nonce as u32).into(),
        Fee {
            gas_limit,
            max_fee_per_gas,
            max_priority_fee_per_gas: U256::from(ecx.tx.gas_priority_fee.unwrap_or_default()),
            gas_per_pubdata_limit: zk_env.gas_per_pubdata().into(),
        },
        caller.to_h160(),
        ecx.tx.value.to_u256(),
        factory_deps.unwrap_or_default(),
        paymaster_params,
    );

    let call_ctx = CallContext {
        tx_caller: ecx.tx.caller,
        msg_sender: ecx.tx.caller,
        contract: transact_to.to_address(),
        input: if is_create { None } else { Some(ecx.tx.data.clone()) },
        delegate_as: None,
        block_number: rU256::from(ecx.block.number),
        block_timestamp: rU256::from(ecx.block.timestamp),
        block_hashes: get_historical_block_hashes(&mut ecx),
        block_basefee: min(max_fee_per_gas.to_ru256(), rU256::from(ecx.block.basefee)),
        is_create,
        is_static: false,
        record_storage_accesses: false,
    };

    let mut ccx = CheatcodeTracerContext {
        persisted_factory_deps,
        zk_env: zk_env.clone(),
        ..Default::default()
    };

    match inspect::<_, DB::Error>(tx, &mut ecx, &mut ccx, call_ctx) {
        Ok(ZKVMExecutionResult { execution_result: result, call_traces, .. }) => Ok((
            ResultAndState { result, state: ecx.journaled_state.finalize().state },
            call_traces,
        )),
        Err(err) => eyre::bail!("zk backend: failed while inspecting: {err:?}"),
    }
}

/// Retrieves L2 ETH balance for a given address.
pub fn balance<DB>(address: Address, ecx: &mut EthEvmContext<DB>) -> rU256
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    let balance = ZKVMData::new(ecx).get_balance(address);
    balance.to_ru256()
}

/// Retrieves bytecode hash stored at a given address.
#[allow(dead_code)]
pub fn code_hash<DB>(address: Address, ecx: &mut EthEvmContext<DB>) -> B256
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    B256::from(ZKVMData::new(ecx).get_code_hash(address).0)
}

/// Retrieves transaction nonce for a given address.
pub fn tx_nonce<DB>(address: Address, ecx: &mut EthEvmContext<DB>) -> u128
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    ZKVMData::new(ecx).get_tx_nonce(address)
}

/// Retrieves deployment nonce for a given address.
pub fn deploy_nonce<DB>(address: Address, ecx: &mut EthEvmContext<DB>) -> u128
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    ZKVMData::new(ecx).get_deploy_nonce(address)
}

/// EraVM equivalent of [`CreateInputs`]
pub struct ZkCreateInputs {
    /// The current `msg.sender`
    pub msg_sender: Address,
    /// The encoded calldata input for `CONTRACT_DEPLOYER`
    pub create_input: Vec<u8>,
    /// Factory deps for the contract we are deploying
    pub factory_deps: Vec<Vec<u8>>,
    /// Value specified for the deployment
    pub value: U256,
}

/// Executes a CREATE opcode on the EraVM.
///
/// * `call.init_code` should be valid EraVM's ContractDeployer input
pub fn create<DB, E>(
    inputs: ZkCreateInputs,
    ecx: &mut EthEvmContext<DB>,
    mut ccx: CheatcodeTracerContext,
) -> ZKVMResult<E>
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    let ZkCreateInputs { create_input, factory_deps, value, msg_sender } = inputs;

    info!("create tx {}", hex::encode(&create_input));
    // We're using `tx.origin` as the initiator so the zkEVM validation does not fail when using
    // `msg.sender` as it's not EOA. The nonce and balance changes thus need to be adapted.
    let caller = ecx.tx.caller;
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
        create_input,
        (nonce as u32).into(),
        Fee {
            gas_limit,
            max_fee_per_gas,
            max_priority_fee_per_gas: U256::from(ecx.tx.gas_priority_fee.unwrap_or_default()),
            gas_per_pubdata_limit: ccx.zk_env.gas_per_pubdata().into(),
        },
        caller.to_h160(),
        value,
        factory_deps,
        paymaster_params,
    );

    let call_ctx = CallContext {
        tx_caller: ecx.tx.caller,
        msg_sender,
        contract: CONTRACT_DEPLOYER_ADDRESS.to_address(),
        input: None,
        delegate_as: None,
        block_number: rU256::from(ecx.block.number),
        block_timestamp: rU256::from(ecx.block.timestamp),
        block_basefee: min(max_fee_per_gas.to_ru256(), rU256::from(ecx.block.basefee)),
        block_hashes: get_historical_block_hashes(ecx),
        is_create: true,
        is_static: false,
        record_storage_accesses: ccx.record_storage_accesses,
    };

    inspect_as_batch(tx, ecx, &mut ccx, call_ctx)
}

/// Executes a CALL opcode on the ZK-VM.
pub fn call<DB, E>(
    call: &CallInputs,
    factory_deps: Vec<Vec<u8>>,
    ecx: &mut EthEvmContext<DB>,
    mut ccx: CheatcodeTracerContext,
) -> ZKVMResult<E>
where
    DB: Database,
    <DB as Database>::Error: Debug,
{
    let input = call.input.bytes(ecx);
    info!(?call, "call tx {}", hex::encode(&input));
    // We're using `tx.origin` as the initiator so the zkEVM validation does not fail when using
    // `msg.sender` as it's not EOA. The nonce and balance changes thus need to be adapted.
    let caller = ecx.tx.caller;
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
        Some(call.bytecode_address.to_h160()),
        input.to_vec(),
        (nonce as u32).into(),
        Fee {
            gas_limit,
            max_fee_per_gas,
            max_priority_fee_per_gas: U256::from(ecx.tx.gas_priority_fee.unwrap_or_default()),
            gas_per_pubdata_limit: ccx.zk_env.gas_per_pubdata().into(),
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
        tx_caller: ecx.tx.caller,
        msg_sender: call.caller,
        contract: call.bytecode_address,
        input: Some(input),
        delegate_as: match call.scheme {
            CallScheme::DelegateCall => Some(call.target_address),
            _ => None,
        },
        block_number: rU256::from(ecx.block.number),
        block_timestamp: rU256::from(ecx.block.timestamp),
        block_hashes: get_historical_block_hashes(ecx),
        block_basefee: min(max_fee_per_gas.to_ru256(), rU256::from(ecx.block.basefee)),
        is_create: false,
        is_static: call.is_static,
        record_storage_accesses: ccx.record_storage_accesses,
    };

    inspect(tx, ecx, &mut ccx, call_ctx)
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
        CreateScheme::Custom { .. } => {
            unimplemented!("Custom create scheme is not supported in foundry-zksync")
        }
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
fn get_historical_block_hashes<DB: Database>(ecx: &mut EthEvmContext<DB>) -> HashMap<rU256, B256> {
    let mut block_hashes = HashMap::default();
    let num_blocks = get_env_historical_block_count();
    tracing::debug!("fetching last {num_blocks} block hashes");
    for i in 1..=num_blocks {
        let (block_number, overflow) = ecx.block.number.overflowing_sub(i as u64);
        if overflow {
            break;
        }
        match ecx.journaled_state.database.block_hash(block_number) {
            Ok(block_hash) => {
                block_hashes.insert(rU256::from(block_number), block_hash);
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
            value
                .parse::<u32>()
                .unwrap_or_else(|err| panic!("failed parsing env variable {name}={value}, {err:?}"))
        })
        .map(|num| num.min(256))
        .unwrap_or(256)
}
