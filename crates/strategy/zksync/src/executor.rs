use std::sync::{Arc, Mutex};

use alloy_primitives::{Address, U256};
use alloy_rpc_types::serde_helpers::OtherFields;
use alloy_zksync::provider::{zksync_provider, ZksyncProvider};
use eyre::Result;
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
use foundry_zksync_core::{vm::ZkEnv, ZkTransactionMetadata};
use revm::{
    primitives::{Env, EnvWithHandlerCfg, HashMap, ResultAndState},
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
    zk_env: ZkEnv,
}

impl ExecutorStrategy for ZksyncExecutorStrategy {
    fn name(&self) -> &'static str {
        "zk"
    }

    fn new_cloned(&self) -> Arc<Mutex<dyn ExecutorStrategy>> {
        Arc::new(Mutex::new(self.clone()))
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
            self.zk_env.clone(),
        )))
    }

    fn call_inspect(
        &self,
        db: &mut dyn DatabaseExt,
        env: &mut EnvWithHandlerCfg,
        inspector: &mut dyn InspectorExt,
    ) -> eyre::Result<ResultAndState> {
        match self.inspect_context.as_ref() {
            None => self.evm.call_inspect(db, env, inspector),
            Some(zk_tx) => foundry_zksync_core::vm::transact(
                Some(&mut self.persisted_factory_deps.clone()),
                Some(zk_tx.factory_deps.clone()),
                zk_tx.paymaster_data.clone(),
                env,
                &self.zk_env,
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
                // since it won't be set by zkEVM
                env.block = executor_env.block.clone();
                env.tx.gas_price = executor_env.tx.gas_price;

                foundry_zksync_core::vm::transact(
                    Some(&mut self.persisted_factory_deps),
                    Some(zk_tx.factory_deps),
                    zk_tx.paymaster_data,
                    env,
                    &self.zk_env,
                    db,
                )
            }
        }
    }
}

impl ExecutorStrategyExt for ZksyncExecutorStrategy {
    fn new_cloned_ext(&self) -> Arc<Mutex<dyn ExecutorStrategyExt>> {
        Arc::new(Mutex::new(self.clone()))
    }

    fn zksync_set_dual_compiled_contracts(
        &mut self,
        dual_compiled_contracts: DualCompiledContracts,
    ) {
        self.dual_compiled_contracts = dual_compiled_contracts;
    }

    fn zksync_set_fork_env(&mut self, fork_url: &str, env: &Env) -> Result<()> {
        let provider = zksync_provider().with_recommended_fillers().on_http(fork_url.parse()?);
        let block_number = env.block.number.try_into()?;
        // TODO(zk): switch to getFeeParams call when it is implemented for anvil-zksync
        let maybe_block_details = tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(provider.get_block_details(block_number))
        })
        .ok()
        .flatten();

        if let Some(block_details) = maybe_block_details {
            self.zk_env = ZkEnv {
                l1_gas_price: block_details
                    .l1_gas_price
                    .try_into()
                    .expect("failed to convert l1_gas_price to u64"),
                fair_l2_gas_price: block_details
                    .l2_fair_gas_price
                    .try_into()
                    .expect("failed to convert fair_l2_gas_price to u64"),
                fair_pubdata_price: block_details
                    .fair_pubdata_price
                    // TODO(zk): None as a value might mean L1Pegged model
                    // we need to find out if it will ever be relevant to
                    // us
                    .unwrap_or_default()
                    .try_into()
                    .expect("failed to convert fair_pubdata_price to u64"),
            };
        }

        Ok(())
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
