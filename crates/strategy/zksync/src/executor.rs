use std::sync::{Arc, Mutex};

use alloy_primitives::{Address, U256};
use alloy_rpc_types::serde_helpers::OtherFields;
use foundry_cheatcodes::strategy::CheatcodeInspectorStrategyExt;

use foundry_evm::{
    backend::{strategy::BackendStrategyExt, BackendResult, DatabaseExt},
    executors::{
        strategy::{EvmExecutorStrategy, ExecutorStrategy, ExecutorStrategyExt},
        Executor,
    },
    InspectorExt,
};
use foundry_zksync_compiler::DualCompiledContracts;
use foundry_zksync_core::ZkTransactionMetadata;
use revm::{
    primitives::{EnvWithHandlerCfg, HashMap, ResultAndState},
    Database,
};
use zksync_types::H256;

use crate::{
    cheatcode::ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY, ZksyncBackendStrategy,
    ZksyncCheatcodeInspectorStrategy,
};

#[derive(Debug, Default, Clone)]
pub struct ZksyncExecutorStrategy {
    evm: EvmExecutorStrategy,
    inspect_context: Option<ZkTransactionMetadata>,
    persisted_factory_deps: HashMap<H256, Vec<u8>>,
    dual_compiled_contracts: DualCompiledContracts,
}

impl ExecutorStrategy for ZksyncExecutorStrategy {
    fn name(&self) -> &'static str {
        "zk"
    }

    fn set_inspect_context(&mut self, other_fields: OtherFields) {
        let maybe_context = get_zksync_transaction_metadata(&other_fields);
        self.inspect_context = maybe_context;
    }

    fn set_balance(
        &mut self,
        executor: &mut Executor,
        address: Address,
        amount: U256,
    ) -> BackendResult<()> {
        self.evm.set_balance(executor, address, amount)?;

        let (address, slot) = foundry_zksync_core::state::get_balance_storage(address);
        executor.backend.insert_account_storage(address, slot, amount)?;

        Ok(())
    }

    fn set_nonce(
        &mut self,
        executor: &mut Executor,
        address: Address,
        nonce: u64,
    ) -> BackendResult<()> {
        self.evm.set_nonce(executor, address, nonce)?;

        let (address, slot) = foundry_zksync_core::state::get_nonce_storage(address);
        // fetch the full nonce to preserve account's deployment nonce
        let full_nonce = executor.backend.storage(address, slot)?;
        let full_nonce = foundry_zksync_core::state::parse_full_nonce(full_nonce);
        let new_full_nonce =
            foundry_zksync_core::state::new_full_nonce(nonce, full_nonce.deploy_nonce);
        executor.backend.insert_account_storage(address, slot, new_full_nonce)?;

        Ok(())
    }

    fn new_backend_strategy(&self) -> Arc<Mutex<dyn BackendStrategyExt>> {
        Arc::new(Mutex::new(ZksyncBackendStrategy::default()))
    }

    fn new_cheatcode_inspector_strategy(&self) -> Arc<Mutex<dyn CheatcodeInspectorStrategyExt>> {
        Arc::new(Mutex::new(ZksyncCheatcodeInspectorStrategy::new(
            self.dual_compiled_contracts.clone(),
        )))
    }

    fn call_inspect(
        &mut self,
        db: &mut dyn DatabaseExt,
        env: &mut EnvWithHandlerCfg,
        inspector: &mut dyn InspectorExt,
    ) -> eyre::Result<ResultAndState> {
        match self.inspect_context.take() {
            None => self.evm.call_inspect(db, env, inspector),
            Some(zk_tx) => foundry_zksync_core::vm::transact(
                Some(&mut self.persisted_factory_deps),
                Some(zk_tx.factory_deps),
                zk_tx.paymaster_data,
                env,
                db,
            ),
        }
    }

    fn transact_inspect(
        &mut self,
        db: &mut dyn DatabaseExt,
        env: &mut EnvWithHandlerCfg,
        executor_env: &EnvWithHandlerCfg,
        inspector: &mut dyn InspectorExt,
    ) -> eyre::Result<ResultAndState> {
        match self.inspect_context.take() {
            None => self.evm.transact_inspect(db, env, executor_env, inspector),
            Some(zk_tx) => {
                // apply fork-related env instead of cheatcode handler
                // since it won't be run inside zkvm
                env.block = executor_env.block.clone();
                env.tx.gas_price = executor_env.tx.gas_price;

                foundry_zksync_core::vm::transact(
                    Some(&mut self.persisted_factory_deps),
                    Some(zk_tx.factory_deps),
                    zk_tx.paymaster_data,
                    env,
                    db,
                )
            }
        }
    }
}

impl ExecutorStrategyExt for ZksyncExecutorStrategy {
    fn zksync_set_dual_compiled_contracts(
        &mut self,
        dual_compiled_contracts: DualCompiledContracts,
    ) {
        self.dual_compiled_contracts = dual_compiled_contracts;
    }
}

/// Retrieve metadata for zksync tx
pub fn get_zksync_transaction_metadata(
    other_fields: &OtherFields,
) -> Option<ZkTransactionMetadata> {
    other_fields
        .get_deserialized::<ZkTransactionMetadata>(ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY)
        .transpose()
        .ok()
        .flatten()
}
