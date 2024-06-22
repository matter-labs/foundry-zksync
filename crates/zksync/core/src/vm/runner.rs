use foundry_zksync_compiler::DualCompiledContract;
use revm::{
    interpreter::{CallInputs, CallScheme, CreateInputs},
    precompile::Precompiles,
    primitives::{
        Address, CreateScheme, Env, ResultAndState, SpecId, TransactTo, B256, U256 as rU256,
    },
    Database, JournaledState,
};
use tracing::{debug, error, info};
use zksync_basic_types::H256;
use zksync_types::{
    ethabi, fee::Fee, l2::L2Tx, transaction_request::PaymasterParams, CONTRACT_DEPLOYER_ADDRESS,
    U256,
};

use std::{cmp::min, fmt::Debug};

use crate::{
    convert::{ConvertAddress, ConvertH160, ConvertRU256, ConvertU256},
    fix_l2_gas_limit, fix_l2_gas_price,
    vm::{
        db::ZKVMData,
        inspect::{inspect, inspect_as_batch, ZKVMExecutionResult, ZKVMResult},
        tracer::{CallContext, CheatcodeTracerContext},
    },
};

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
    debug!("zk transact");
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
    info!(?gas_limit, ?max_fee_per_gas, "tx gas parameters");
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

    match inspect::<_, DB::Error>(
        tx,
        env,
        db,
        &mut journaled_state,
        &mut Default::default(),
        call_ctx,
    ) {
        Ok(ZKVMExecutionResult { execution_result: result, .. }) => {
            Ok(ResultAndState { result, state: journaled_state.finalize().0 })
        }
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
    mut ccx: CheatcodeTracerContext,
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
    info!(?gas_limit, ?max_fee_per_gas, "tx gas parameters");

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

    inspect_as_batch(tx, env, db, journaled_state, &mut ccx, call_ctx)
}

/// Executes a CALL opcode on the ZK-VM.
pub fn call<'a, DB, E>(
    call: &CallInputs,
    env: &'a mut Env,
    db: &'a mut DB,
    journaled_state: &'a mut JournaledState,
    mut ccx: CheatcodeTracerContext,
) -> ZKVMResult<E>
where
    DB: Database + Send,
    <DB as Database>::Error: Debug,
{
    info!(?call, "call tx {}", hex::encode(&call.input));
    let caller = env.tx.caller;
    let nonce: zksync_types::Nonce = ZKVMData::new(db, journaled_state).get_tx_nonce(caller);

    let (gas_limit, max_fee_per_gas) = gas_params(env, db, journaled_state, caller);
    info!(?gas_limit, ?max_fee_per_gas, "tx gas parameters");
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

    inspect(tx, env, db, journaled_state, &mut ccx, call_ctx)
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
        error!("balance is 0 for {caller:?}, transaction will fail");
    }
    let max_fee_per_gas = fix_l2_gas_price(env.tx.gas_price.to_u256());
    let gas_limit = fix_l2_gas_limit(env.tx.gas_limit.into(), max_fee_per_gas, value, balance);

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
