use std::any::TypeId;

use alloy_eips::eip2718::Decodable2718;
use alloy_primitives::U256;
use alloy_rpc_types::TransactionRequest;
use alloy_sol_types::SolValue;
use alloy_zksync::network::tx_envelope::TxEnvelope as ZkTxEnvelope;
use eyre::Context;
use foundry_cheatcodes::{
    BroadcastableTransaction, CheatcodesExecutor, CheatsCtxt, DealRecord, DynCheatcode, Error,
    Result,
    Vm::{
        broadcastRawTransactionCall, createFork_0Call, createFork_1Call, createFork_2Call,
        createSelectFork_0Call, createSelectFork_1Call, createSelectFork_2Call, dealCall, etchCall,
        getCodeCall, getNonce_0Call, mockCall_0Call, mockCallRevert_0Call, resetNonceCall,
        rollCall, selectForkCall, setNonceCall, setNonceUnsafeCall, warpCall,
        zkGetDeploymentNonceCall, zkGetTransactionNonceCall, zkRegisterContractCall,
        zkUseFactoryDepCall, zkUsePaymasterCall, zkVmCall, zkVmSkipCall,
    },
    make_acc_non_empty,
};
use foundry_common::TransactionMaybeSigned;
use foundry_compilers::info::ContractInfo;
use foundry_evm::backend::LocalForkId;
use foundry_evm_core::ContextExt;
use foundry_zksync_compilers::dual_compiled_contracts::DualCompiledContract;
use foundry_zksync_core::{
    H256, PaymasterParams, ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY, ZkPaymasterData,
    ZkTransactionMetadata,
};
use revm::interpreter::InstructionResult;
use tracing::{info, warn};

use crate::{
    backend::ZksyncInspectContext,
    cheatcode::{
        ZksyncCheatcodeInspectorStrategyRunner,
        runner::{WithOtherFields, get_context, utils::get_artifact_code},
    },
};

impl ZksyncCheatcodeInspectorStrategyRunner {
    pub(super) fn apply_cheatcode_impl(
        &self,
        cheatcode: &dyn DynCheatcode,
        ccx: &mut CheatsCtxt<'_, '_, '_, '_>,
        executor: &mut dyn CheatcodesExecutor,
    ) -> Result {
        fn is<T: std::any::Any>(t: TypeId) -> bool {
            TypeId::of::<T>() == t
        }

        let using_zk_vm = get_context(ccx.state.strategy.context.as_mut()).using_zk_vm;

        // Try to downcast the cheatcode to a type that requires special handling.
        // Note that some cheatcodes are only handled in zkEVM context.
        // If no handler fires, we use the default execution logic.
        match cheatcode.as_any().type_id() {
            t if using_zk_vm && is::<etchCall>(t) => {
                let etchCall { target, newRuntimeBytecode } =
                    cheatcode.as_any().downcast_ref().unwrap();
                foundry_zksync_core::cheatcodes::etch(*target, newRuntimeBytecode, ccx.ecx);
                Ok(Default::default())
            }
            t if using_zk_vm && is::<rollCall>(t) => {
                let &rollCall { newHeight } = cheatcode.as_any().downcast_ref().unwrap();
                foundry_zksync_core::cheatcodes::roll(newHeight, ccx.ecx);
                Ok(Default::default())
            }
            t if using_zk_vm && is::<warpCall>(t) => {
                let &warpCall { newTimestamp } = cheatcode.as_any().downcast_ref().unwrap();
                foundry_zksync_core::cheatcodes::warp(newTimestamp, ccx.ecx);
                Ok(Default::default())
            }
            t if using_zk_vm && is::<dealCall>(t) => {
                let &dealCall { account, newBalance } = cheatcode.as_any().downcast_ref().unwrap();

                let old_balance =
                    foundry_zksync_core::cheatcodes::deal(account, newBalance, ccx.ecx);
                let record = DealRecord { address: account, old_balance, new_balance: newBalance };
                ccx.state.eth_deals.push(record);
                Ok(Default::default())
            }
            t if using_zk_vm && is::<resetNonceCall>(t) => {
                let &resetNonceCall { account } = cheatcode.as_any().downcast_ref().unwrap();
                foundry_zksync_core::cheatcodes::set_nonce(account, U256::ZERO, ccx.ecx);
                Ok(Default::default())
            }
            t if using_zk_vm && is::<setNonceCall>(t) => {
                let &setNonceCall { account, newNonce } =
                    cheatcode.as_any().downcast_ref().unwrap();

                // nonce must increment only
                let current = foundry_zksync_core::cheatcodes::get_nonce(account, ccx.ecx);
                if U256::from(newNonce) < current {
                    return Err(Error::display(format!(
                        "new nonce ({newNonce}) must be strictly equal to or higher than the \
                account's current nonce ({current})"
                    )));
                }

                foundry_zksync_core::cheatcodes::set_nonce(account, U256::from(newNonce), ccx.ecx);
                Ok(Default::default())
            }
            t if using_zk_vm && is::<setNonceUnsafeCall>(t) => {
                let &setNonceUnsafeCall { account, newNonce } =
                    cheatcode.as_any().downcast_ref().unwrap();
                foundry_zksync_core::cheatcodes::set_nonce(account, U256::from(newNonce), ccx.ecx);
                Ok(Default::default())
            }
            t if using_zk_vm && is::<getNonce_0Call>(t) => {
                let &getNonce_0Call { account } = cheatcode.as_any().downcast_ref().unwrap();

                let nonce = foundry_zksync_core::cheatcodes::get_nonce(account, ccx.ecx);
                Ok(nonce.abi_encode())
            }
            t if using_zk_vm && is::<zkGetTransactionNonceCall>(t) => {
                let &zkGetTransactionNonceCall { account } =
                    cheatcode.as_any().downcast_ref().unwrap();

                info!(?account, "cheatcode zkGetTransactionNonce");

                let (tx_nonce, _) =
                    foundry_zksync_core::cheatcodes::get_full_nonce(account, ccx.ecx);
                Ok(tx_nonce.abi_encode())
            }
            t if using_zk_vm && is::<zkGetDeploymentNonceCall>(t) => {
                let &zkGetDeploymentNonceCall { account } =
                    cheatcode.as_any().downcast_ref().unwrap();

                info!(?account, "cheatcode zkGetDeploymentNonce");

                let (_, deploy_nonce) =
                    foundry_zksync_core::cheatcodes::get_full_nonce(account, ccx.ecx);
                Ok(deploy_nonce.abi_encode())
            }
            t if using_zk_vm && is::<mockCall_0Call>(t) => {
                let mockCall_0Call { callee, data, returnData } =
                    cheatcode.as_any().downcast_ref().unwrap();

                let _ = make_acc_non_empty(callee, ccx)?;
                foundry_zksync_core::cheatcodes::set_mocked_account(*callee, ccx.ecx, ccx.caller);
                foundry_cheatcodes::mock_call(
                    ccx.state,
                    callee,
                    data,
                    None,
                    returnData,
                    InstructionResult::Return,
                );
                Ok(Default::default())
            }
            t if using_zk_vm && is::<mockCallRevert_0Call>(t) => {
                let mockCallRevert_0Call { callee, data, revertData } =
                    cheatcode.as_any().downcast_ref().unwrap();

                let _ = make_acc_non_empty(callee, ccx)?;
                foundry_zksync_core::cheatcodes::set_mocked_account(*callee, ccx.ecx, ccx.caller);
                // not calling
                foundry_cheatcodes::mock_call(
                    ccx.state,
                    callee,
                    data,
                    None,
                    revertData,
                    InstructionResult::Revert,
                );
                Ok(Default::default())
            }
            t if is::<getCodeCall>(t) => {
                // We don't need to check for `using_zk_vm` since we pass it to `get_artifact_code`.
                let getCodeCall { artifactPath } = cheatcode.as_any().downcast_ref().unwrap();

                let ctx = get_context(ccx.state.strategy.context.as_mut());

                Ok(get_artifact_code(
                    &ctx.dual_compiled_contracts,
                    ctx.using_zk_vm,
                    &ccx.state.config,
                    artifactPath,
                    false,
                )?
                .abi_encode())
            }
            t if is::<zkVmCall>(t) => {
                let zkVmCall { enable } = cheatcode.as_any().downcast_ref().unwrap();
                let ctx = get_context(ccx.state.strategy.context.as_mut());
                if *enable {
                    self.select_zk_vm(ctx, ccx.ecx, None)
                } else {
                    self.select_evm(ctx, ccx.ecx);
                }
                Ok(Default::default())
            }
            t if is::<zkVmSkipCall>(t) => {
                let zkVmSkipCall { .. } = cheatcode.as_any().downcast_ref().unwrap();
                let ctx = get_context(ccx.state.strategy.context.as_mut());
                ctx.skip_zk_vm = true;
                Ok(Default::default())
            }
            t if is::<zkUsePaymasterCall>(t) => {
                let zkUsePaymasterCall { paymaster_address, paymaster_input } =
                    cheatcode.as_any().downcast_ref().unwrap();
                let ctx = get_context(ccx.state.strategy.context.as_mut());
                ctx.paymaster_params = Some(ZkPaymasterData {
                    address: *paymaster_address,
                    input: paymaster_input.clone(),
                });
                Ok(Default::default())
            }
            t if is::<zkUseFactoryDepCall>(t) => {
                let zkUseFactoryDepCall { name } = cheatcode.as_any().downcast_ref().unwrap();
                info!("Adding factory dependency: {:?}", name);
                let ctx = get_context(ccx.state.strategy.context.as_mut());
                ctx.zk_use_factory_deps.push(name.clone());
                Ok(Default::default())
            }
            t if is::<zkRegisterContractCall>(t) => {
                let zkRegisterContractCall {
                    name,
                    evmBytecodeHash,
                    evmDeployedBytecode,
                    evmBytecode,
                    zkBytecodeHash,
                    zkDeployedBytecode,
                } = cheatcode.as_any().downcast_ref().unwrap();
                let ctx = get_context(ccx.state.strategy.context.as_mut());

                let zk_factory_deps = vec![]; //TODO: add argument to cheatcode
                let new_contract_info = ContractInfo::new(name);
                let new_contract = DualCompiledContract {
                    zk_bytecode_hash: H256(zkBytecodeHash.0),
                    zk_deployed_bytecode: zkDeployedBytecode.to_vec(),
                    zk_factory_deps,
                    evm_bytecode_hash: *evmBytecodeHash,
                    evm_deployed_bytecode: evmDeployedBytecode.to_vec(),
                    evm_immutable_references: None, // TODO(zk): Is it possible to get this info?
                    evm_bytecode: evmBytecode.to_vec(),
                };

                if let Some((existing, _)) =
                    ctx.dual_compiled_contracts.iter().find(|(_, contract)| {
                        contract.evm_bytecode_hash == new_contract.evm_bytecode_hash
                            && contract.zk_bytecode_hash == new_contract.zk_bytecode_hash
                    })
                {
                    warn!(
                        name = existing.name,
                        "contract already exists with the given bytecode hashes"
                    );
                    return Ok(Default::default());
                }

                ctx.dual_compiled_contracts.insert(new_contract_info, new_contract);

                Ok(Default::default())
            }
            t if is::<selectForkCall>(t) => {
                let selectForkCall { forkId } = cheatcode.as_any().downcast_ref().unwrap();
                let ctx = get_context(ccx.state.strategy.context.as_mut());

                // Re-implementation of `persist_caller` from `fork.rs`.
                ccx.ecx.journaled_state.database.add_persistent_account(ccx.caller);

                // Prepare storage.
                self.select_fork_vm(ctx, ccx.ecx, *forkId);

                // Apply cheatcode as usual.
                cheatcode.dyn_apply(ccx, executor)
            }
            t if is::<createSelectFork_0Call>(t) => {
                let createSelectFork_0Call { urlOrAlias } =
                    cheatcode.as_any().downcast_ref().unwrap();

                // Re-implementation of `persist_caller` from `fork.rs`.
                ccx.ecx.journaled_state.database.add_persistent_account(ccx.caller);

                // Create fork.
                let create_fork_cheatcode = createFork_0Call { urlOrAlias: urlOrAlias.clone() };

                let encoded_fork_id = create_fork_cheatcode.dyn_apply(ccx, executor)?;
                let fork_id = LocalForkId::abi_decode(&encoded_fork_id)?;

                // Prepare storage.
                {
                    let ctx = get_context(ccx.state.strategy.context.as_mut());
                    self.select_fork_vm(ctx, ccx.ecx, fork_id);
                }

                // Select fork
                let select_fork_cheatcode = selectForkCall { forkId: fork_id };
                select_fork_cheatcode.dyn_apply(ccx, executor)?;

                // We need to return the fork ID.
                Ok(encoded_fork_id)
            }
            t if is::<createSelectFork_1Call>(t) => {
                let createSelectFork_1Call { urlOrAlias, blockNumber } =
                    cheatcode.as_any().downcast_ref().unwrap();

                // Re-implementation of `persist_caller` from `fork.rs`.
                ccx.ecx.journaled_state.database.add_persistent_account(ccx.caller);

                // Create fork.
                let create_fork_cheatcode =
                    createFork_1Call { urlOrAlias: urlOrAlias.clone(), blockNumber: *blockNumber };
                let encoded_fork_id = create_fork_cheatcode.dyn_apply(ccx, executor)?;
                let fork_id = LocalForkId::abi_decode(&encoded_fork_id)?;

                // Prepare storage.
                {
                    let ctx = get_context(ccx.state.strategy.context.as_mut());
                    self.select_fork_vm(ctx, ccx.ecx, fork_id);
                }

                // Select fork
                let select_fork_cheatcode = selectForkCall { forkId: fork_id };
                select_fork_cheatcode.dyn_apply(ccx, executor)?;

                // We need to return the fork ID.
                Ok(encoded_fork_id)
            }
            t if is::<createSelectFork_2Call>(t) => {
                let createSelectFork_2Call { urlOrAlias, txHash } =
                    cheatcode.as_any().downcast_ref().unwrap();

                // Re-implementation of `persist_caller` from `fork.rs`.
                ccx.ecx.journaled_state.database.add_persistent_account(ccx.caller);

                // Create fork.
                let create_fork_cheatcode =
                    createFork_2Call { urlOrAlias: urlOrAlias.clone(), txHash: *txHash };
                let encoded_fork_id = create_fork_cheatcode.dyn_apply(ccx, executor)?;
                let fork_id = LocalForkId::abi_decode(&encoded_fork_id)?;

                // Prepare storage.
                {
                    let ctx = get_context(ccx.state.strategy.context.as_mut());
                    self.select_fork_vm(ctx, ccx.ecx, fork_id);
                }

                // Select fork
                let select_fork_cheatcode = selectForkCall { forkId: fork_id };
                select_fork_cheatcode.dyn_apply(ccx, executor)?;

                // We need to return the fork ID.
                Ok(encoded_fork_id)
            }
            t if using_zk_vm && is::<broadcastRawTransactionCall>(t) => {
                let broadcastRawTransactionCall { data } =
                    cheatcode.as_any().downcast_ref().unwrap();

                let envelope = ZkTxEnvelope::decode_2718(&mut data.as_ref())
                    .wrap_err("Failed to decode tx")?;
                let parts = match envelope {
                    ZkTxEnvelope::Eip712(signed) => Ok(signed.into_parts().0),
                    _ => Err(Error::display("not a valid eip712 tx")),
                }?;

                let tx: TransactionRequest = TransactionRequest {
                    from: Some(parts.from),
                    max_fee_per_gas: Some(parts.max_fee_per_gas),
                    max_priority_fee_per_gas: Some(parts.max_priority_fee_per_gas),
                    max_fee_per_blob_gas: Default::default(),
                    gas: Some(parts.gas),
                    input: Some(parts.input).into(),
                    chain_id: Some(parts.chain_id),
                    access_list: Default::default(),
                    transaction_type: Default::default(),
                    blob_versioned_hashes: Default::default(),
                    sidecar: Default::default(),
                    authorization_list: Default::default(),
                    to: Some(alloy_primitives::TxKind::Call(parts.to)),
                    value: Some(parts.value),
                    nonce: Some(parts.nonce.as_limbs()[0]),
                    gas_price: Default::default(),
                };

                let (factory_deps, paymaster_data) = match parts.eip712_meta {
                    None => Default::default(),
                    Some(meta) => (
                        meta.factory_deps.into_iter().map(|b| b.to_vec()).collect::<Vec<_>>(),
                        meta.paymaster_params.map(|params| PaymasterParams {
                            paymaster: zksync_types::H160::from(params.paymaster.0.0),
                            paymaster_input: params.paymaster_input.to_vec(),
                        }),
                    ),
                };

                let inspect_ctx = ZksyncInspectContext {
                    factory_deps: factory_deps.clone(),
                    paymaster_data: paymaster_data.clone(),
                    zk_env: get_context(ccx.state.strategy.context.as_mut()).zk_env.clone(),
                };

                let (db, journal, env) = ccx.ecx.as_db_env_and_journal();
                db.transact_from_tx(
                    &tx,
                    env.to_owned(),
                    journal,
                    &mut *executor.get_inspector(ccx.state),
                    Box::new(inspect_ctx),
                )?;

                let mut tx_with_fields = WithOtherFields::new(tx);
                tx_with_fields.other.insert(
                    ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY.to_string(),
                    serde_json::to_value(ZkTransactionMetadata::new(factory_deps, paymaster_data))
                        .expect("failed encoding json"),
                );

                if ccx.state.broadcast.is_some() {
                    ccx.state.broadcastable_transactions.push_back(BroadcastableTransaction {
                        rpc: ccx.journaled_state.database.active_fork_url(),
                        transaction: { TransactionMaybeSigned::new(tx_with_fields) },
                    });
                }
                Ok(Default::default())
            }
            _ => {
                // Not custom, just invoke the default behavior
                cheatcode.dyn_apply(ccx, executor)
            }
        }
    }
}
