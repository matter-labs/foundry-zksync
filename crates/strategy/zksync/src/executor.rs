use std::sync::{Arc, Mutex};

use alloy_primitives::{Address, U256};
use foundry_cheatcodes::strategy::CheatcodeInspectorStrategyExt;
use foundry_evm::{
    backend::{strategy::BackendStrategy, BackendResult},
    executors::{
        strategy::{EvmExecutorStrategy, ExecutorStrategy},
        Executor,
    },
};
use foundry_zksync_compiler::DualCompiledContracts;
use revm::Database;

use crate::{ZksyncBackendStrategy, ZksyncCheatcodeInspectorStrategy};

#[derive(Debug, Default, Clone)]
pub struct ZksyncExecutorStrategy {
    evm: EvmExecutorStrategy,
}

impl ExecutorStrategy for ZksyncExecutorStrategy {
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

    fn new_backend_strategy(&self) -> Arc<Mutex<dyn BackendStrategy>> {
        Arc::new(Mutex::new(ZksyncBackendStrategy::default()))
    }

    fn new_cheatcode_inspector_strategy(
        &self,
        dual_compiled_contracts: DualCompiledContracts,
    ) -> Arc<Mutex<dyn CheatcodeInspectorStrategyExt>> {
        Arc::new(Mutex::new(ZksyncCheatcodeInspectorStrategy::new(dual_compiled_contracts)))
    }
}