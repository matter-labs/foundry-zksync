use std::path::Path;

use alloy_primitives::{Address, U256};
use alloy_rpc_types::serde_helpers::OtherFields;
use alloy_zksync::provider::{zksync_provider, ZksyncProvider};
use eyre::Result;
use foundry_linking::LinkerError;
use revm::{
    primitives::{Env, EnvWithHandlerCfg, ResultAndState},
    Database,
};

use foundry_compilers::ProjectCompileOutput;
use foundry_config::Config;
use foundry_evm::{
    backend::{Backend, BackendResult, CowBackend},
    decode::RevertDecoder,
    executors::{
        strategy::{
            DeployLibKind, DeployLibResult, EvmExecutorStrategyRunner, ExecutorStrategyContext,
            ExecutorStrategyExt, ExecutorStrategyRunner, LinkOutput,
        },
        EvmError, Executor,
    },
    inspectors::InspectorStack,
};
use foundry_zksync_compilers::{
    compilers::{artifact_output::zk::ZkArtifactOutput, zksolc::ZkSolcCompiler},
    dual_compiled_contracts::DualCompiledContracts,
};
use foundry_zksync_core::vm::ZkEnv;

use crate::{
    backend::{ZksyncBackendStrategyBuilder, ZksyncInspectContext},
    cheatcode::ZksyncCheatcodeInspectorStrategyBuilder,
    executor::{try_get_zksync_transaction_metadata, ZksyncExecutorStrategyContext},
};

mod libraries;

/// Defines the [ExecutorStrategyRunner] strategy for ZKsync.
#[derive(Debug, Default, Clone)]
pub struct ZksyncExecutorStrategyRunner;

impl ZksyncExecutorStrategyRunner {
    fn set_deployment_nonce(
        executor: &mut Executor,
        address: Address,
        nonce: u64,
    ) -> BackendResult<()> {
        let (address, slot) = foundry_zksync_core::state::get_nonce_storage(address);
        // fetch the full nonce to preserve account's tx nonce
        let full_nonce = executor.backend.storage(address, slot)?;
        let full_nonce = foundry_zksync_core::state::parse_full_nonce(full_nonce);
        let new_full_nonce =
            foundry_zksync_core::state::new_full_nonce(full_nonce.tx_nonce, nonce as u128);
        executor.backend.insert_account_storage(address, slot, new_full_nonce)?;

        Ok(())
    }
}

fn get_context_ref(ctx: &dyn ExecutorStrategyContext) -> &ZksyncExecutorStrategyContext {
    ctx.as_any_ref().downcast_ref().expect("expected ZksyncExecutorStrategyContext")
}

fn get_context(ctx: &mut dyn ExecutorStrategyContext) -> &mut ZksyncExecutorStrategyContext {
    ctx.as_any_mut().downcast_mut().expect("expected ZksyncExecutorStrategyContext")
}

impl ExecutorStrategyRunner for ZksyncExecutorStrategyRunner {
    fn set_balance(
        &self,
        executor: &mut Executor,
        address: Address,
        amount: U256,
    ) -> BackendResult<()> {
        EvmExecutorStrategyRunner.set_balance(executor, address, amount)?;

        let (address, slot) = foundry_zksync_core::state::get_balance_storage(address);
        executor.backend.insert_account_storage(address, slot, amount)?;

        Ok(())
    }

    fn get_balance(&self, executor: &mut Executor, address: Address) -> BackendResult<U256> {
        let (address, slot) = foundry_zksync_core::state::get_balance_storage(address);
        let balance = executor.backend.storage(address, slot)?;

        Ok(balance)
    }

    fn set_nonce(
        &self,
        executor: &mut Executor,
        address: Address,
        nonce: u64,
    ) -> BackendResult<()> {
        EvmExecutorStrategyRunner.set_nonce(executor, address, nonce)?;

        let (address, slot) = foundry_zksync_core::state::get_nonce_storage(address);
        // fetch the full nonce to preserve account's deployment nonce
        let full_nonce = executor.backend.storage(address, slot)?;
        let full_nonce = foundry_zksync_core::state::parse_full_nonce(full_nonce);
        let new_full_nonce =
            foundry_zksync_core::state::new_full_nonce(nonce as u128, full_nonce.deploy_nonce);
        executor.backend.insert_account_storage(address, slot, new_full_nonce)?;

        Ok(())
    }

    fn get_nonce(&self, executor: &mut Executor, address: Address) -> BackendResult<u64> {
        let (address, slot) = foundry_zksync_core::state::get_nonce_storage(address);
        let full_nonce = executor.backend.storage(address, slot)?;
        let full_nonce = foundry_zksync_core::state::parse_full_nonce(full_nonce);

        Ok(full_nonce.tx_nonce as u64)
    }

    fn link(
        &self,
        ctx: &mut dyn ExecutorStrategyContext,
        config: &Config,
        root: &Path,
        input: &ProjectCompileOutput,
        deployer: Address,
    ) -> Result<LinkOutput, LinkerError> {
        self.link_impl(ctx, config, root, input, deployer)
    }

    fn deploy_library(
        &self,
        executor: &mut Executor,
        from: Address,
        kind: DeployLibKind,
        value: U256,
        rd: Option<&RevertDecoder>,
    ) -> Result<Vec<DeployLibResult>, EvmError> {
        self.deploy_library_impl(executor, from, kind, value, rd)
    }

    fn new_backend_strategy(&self) -> foundry_evm_core::backend::strategy::BackendStrategy {
        foundry_evm_core::backend::strategy::BackendStrategy::new_zksync()
    }

    fn new_cheatcode_inspector_strategy(
        &self,
        ctx: &dyn ExecutorStrategyContext,
    ) -> foundry_cheatcodes::strategy::CheatcodeInspectorStrategy {
        let ctx = get_context_ref(ctx);
        foundry_cheatcodes::strategy::CheatcodeInspectorStrategy::new_zksync(
            ctx.dual_compiled_contracts.clone(),
            ctx.zk_env.clone(),
        )
    }

    fn call(
        &self,
        ctx: &dyn ExecutorStrategyContext,
        backend: &mut CowBackend<'_>,
        env: &mut EnvWithHandlerCfg,
        executor_env: &EnvWithHandlerCfg,
        inspector: &mut InspectorStack,
    ) -> eyre::Result<ResultAndState> {
        let ctx = get_context_ref(ctx);

        match ctx.transaction_context.as_ref() {
            None => EvmExecutorStrategyRunner.call(ctx, backend, env, executor_env, inspector),
            Some(zk_tx) => backend.inspect(
                env,
                inspector,
                Box::new(ZksyncInspectContext {
                    factory_deps: zk_tx.factory_deps.clone(),
                    paymaster_data: zk_tx.paymaster_data.clone(),
                    zk_env: ctx.zk_env.clone(),
                }),
            ),
        }
    }

    fn transact(
        &self,
        ctx: &mut dyn ExecutorStrategyContext,
        backend: &mut Backend,
        env: &mut EnvWithHandlerCfg,
        executor_env: &EnvWithHandlerCfg,
        inspector: &mut InspectorStack,
    ) -> eyre::Result<ResultAndState> {
        let ctx = get_context(ctx);

        match ctx.transaction_context.take() {
            None => EvmExecutorStrategyRunner.transact(ctx, backend, env, executor_env, inspector),
            Some(zk_tx) => {
                // apply fork-related env instead of cheatcode handler
                // since it won't be set by zkEVM
                env.block = executor_env.block.clone();
                env.tx.gas_price = executor_env.tx.gas_price;

                backend.inspect(
                    env,
                    inspector,
                    Box::new(ZksyncInspectContext {
                        factory_deps: zk_tx.factory_deps,
                        paymaster_data: zk_tx.paymaster_data,
                        zk_env: ctx.zk_env.clone(),
                    }),
                )
            }
        }
    }
}

impl ExecutorStrategyExt for ZksyncExecutorStrategyRunner {
    fn zksync_set_dual_compiled_contracts(
        &self,
        ctx: &mut dyn ExecutorStrategyContext,
        dual_compiled_contracts: DualCompiledContracts,
    ) {
        let ctx = get_context(ctx);
        ctx.dual_compiled_contracts = dual_compiled_contracts;
    }

    fn zksync_set_compilation_output(
        &self,
        ctx: &mut dyn ExecutorStrategyContext,
        output: ProjectCompileOutput<ZkSolcCompiler, ZkArtifactOutput>,
    ) {
        let ctx = get_context(ctx);
        ctx.compilation_output.replace(output);
    }

    fn zksync_set_fork_env(
        &self,
        ctx: &mut dyn ExecutorStrategyContext,
        fork_url: &str,
        env: &Env,
    ) -> Result<()> {
        let ctx = get_context(ctx);

        let provider = zksync_provider().with_recommended_fillers().on_http(fork_url.parse()?);
        let block_number = env.block.number.try_into()?;
        // TODO(zk): switch to getFeeParams call when it is implemented for anvil-zksync
        let maybe_block_details = tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(provider.get_block_details(block_number))
        })
        .ok()
        .flatten();

        if let Some(block_details) = maybe_block_details {
            ctx.zk_env = ZkEnv {
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

    fn zksync_set_transaction_context(
        &self,
        ctx: &mut dyn ExecutorStrategyContext,
        other_fields: OtherFields,
    ) {
        let ctx = get_context(ctx);
        let transaction_context = try_get_zksync_transaction_metadata(&other_fields);
        ctx.transaction_context = transaction_context;
    }
}
