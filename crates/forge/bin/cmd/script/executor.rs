use crate::cmd::script::transaction::ZkTransaction;

use super::{
    artifacts::ArtifactInfo,
    runner::{ScriptRunner, SimulationStage},
    transaction::{AdditionalContract, TransactionWithMetadata},
    ScriptArgs, ScriptConfig, ScriptResult,
};
use alloy_primitives::{Address, Bytes, U256};
use eyre::{Context, Result};
use forge::{
    backend::Backend,
    executors::ExecutorBuilder,
    inspectors::{cheatcodes::BroadcastableTransactions, CheatsConfig},
    traces::{render_trace_arena, CallTraceDecoder},
};
use foundry_cli::utils::{ensure_clean_constructor, needs_setup};
use foundry_common::{get_contract_name, provider::ethers::RpcUrl, shell, ContractsByArtifact};
use foundry_compilers::artifacts::ContractBytecodeSome;
use foundry_evm::inspectors::cheatcodes::ScriptWallets;
use foundry_zksync_compiler::DualCompiledContracts;
use futures::future::join_all;
use parking_lot::RwLock;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::Arc,
};

impl ScriptArgs {
    /// Locally deploys and executes the contract method that will collect all broadcastable
    /// transactions.
    pub async fn execute(
        &self,
        script_config: &mut ScriptConfig,
        contract: ContractBytecodeSome,
        sender: Address,
        predeploy_libraries: &[Bytes],
        script_wallets: ScriptWallets,
        dual_compiled_contracts: Option<DualCompiledContracts>,
    ) -> Result<ScriptResult> {
        trace!(target: "script", "start executing script");

        let ContractBytecodeSome { abi, bytecode, .. } = contract;

        let bytecode = bytecode.into_bytes().ok_or_else(|| {
            eyre::eyre!("expected fully linked bytecode, found unlinked bytecode")
        })?;

        ensure_clean_constructor(&abi)?;

        let mut runner = self
            .prepare_runner(
                script_config,
                sender,
                SimulationStage::Local,
                Some(script_wallets),
                dual_compiled_contracts,
            )
            .await?;
        let (address, mut result) = runner.setup(
            predeploy_libraries,
            bytecode,
            needs_setup(&abi),
            script_config.sender_nonce,
            self.broadcast,
            script_config.evm_opts.fork_url.is_none(),
        )?;

        let (func, calldata) = self.get_method_and_calldata(&abi)?;
        script_config.called_function = Some(func);

        // Only call the method if `setUp()` succeeded.
        if result.success {
            let script_result = runner.script(address, calldata)?;

            result.success &= script_result.success;
            result.gas_used = script_result.gas_used;
            result.logs.extend(script_result.logs);
            result.traces.extend(script_result.traces);
            result.debug = script_result.debug;
            result.labeled_addresses.extend(script_result.labeled_addresses);
            result.returned = script_result.returned;
            result.breakpoints = script_result.breakpoints;

            match (&mut result.transactions, script_result.transactions) {
                (Some(txs), Some(new_txs)) => {
                    txs.extend(new_txs);
                }
                (None, Some(new_txs)) => {
                    result.transactions = Some(new_txs);
                }
                _ => {}
            }
        }

        Ok(result)
    }

    /// Simulates onchain state by executing a list of transactions locally and persisting their
    /// state. Returns the transactions and any CREATE2 contract address created.
    pub async fn onchain_simulation(
        &self,
        transactions: BroadcastableTransactions,
        script_config: &ScriptConfig,
        decoder: &CallTraceDecoder,
        contracts: &ContractsByArtifact,
        dual_compiled_contracts: Option<DualCompiledContracts>,
    ) -> Result<VecDeque<TransactionWithMetadata>> {
        trace!(target: "script", "executing onchain simulation");

        let runners = Arc::new(
            self.build_runners(script_config, dual_compiled_contracts)
                .await?
                .into_iter()
                .map(|(rpc, runner)| (rpc, Arc::new(RwLock::new(runner))))
                .collect::<HashMap<_, _>>(),
        );

        if script_config.evm_opts.verbosity > 3 {
            println!("==========================");
            println!("Simulated On-chain Traces:\n");
        }

        let address_to_abi: BTreeMap<Address, ArtifactInfo> = decoder
            .contracts
            .iter()
            .filter_map(|(addr, contract_id)| {
                let contract_name = get_contract_name(contract_id);
                if let Ok(Some((_, (abi, code)))) =
                    contracts.find_by_name_or_identifier(contract_name)
                {
                    let info = ArtifactInfo {
                        contract_name: contract_name.to_string(),
                        contract_id: contract_id.to_string(),
                        abi,
                        code,
                    };
                    return Some((*addr, info));
                }
                None
            })
            .collect();

        let mut final_txs = VecDeque::new();

        // Executes all transactions from the different forks concurrently.
        let futs = transactions
            .into_iter()
            .map(|transaction| async {
                let rpc = transaction.rpc.as_ref().expect("missing broadcastable tx rpc url");
                let mut runner = runners.get(rpc).expect("invalid rpc url").write();

                let zk = transaction.zk_tx;
                let mut tx = transaction.transaction;
                let result = runner
                    .simulate(
                        tx.from
                            .expect("transaction doesn't have a `from` address at execution time"),
                        tx.to,
                        tx.input.clone().into_input(),
                        tx.value,
                        (script_config.config.zksync.run_in_zk_mode(), zk.clone()),
                    )
                    .wrap_err("Internal EVM error during simulation")?;

                if !result.success || result.traces.is_empty() {
                    return Ok((None, result.traces));
                }

                let created_contracts = result
                    .traces
                    .iter()
                    .flat_map(|(_, traces)| {
                        traces.nodes().iter().filter_map(|node| {
                            if node.trace.kind.is_any_create() {
                                return Some(AdditionalContract {
                                    opcode: node.trace.kind,
                                    address: node.trace.address,
                                    init_code: node.trace.data.clone(),
                                });
                            }
                            None
                        })
                    })
                    .collect();

                // Simulate mining the transaction if the user passes `--slow`.
                if self.slow {
                    runner.executor.env.block.number += U256::from(1);
                }

                let is_fixed_gas_limit = tx.gas.is_some();
                match tx.gas {
                    // If tx.gas is already set that means it was specified in script
                    Some(gas) => {
                        println!("Gas limit was set in script to {gas}");
                    }
                    // We inflate the gas used by the user specified percentage
                    None => {
                        let gas = U256::from(result.gas_used * self.gas_estimate_multiplier / 100);
                        tx.gas = Some(gas);
                    }
                }

                let tx = TransactionWithMetadata::new_with_zk(
                    tx,
                    transaction.rpc,
                    &result,
                    &address_to_abi,
                    decoder,
                    created_contracts,
                    is_fixed_gas_limit,
                    zk.map(|zk_tx| ZkTransaction { factory_deps: zk_tx.factory_deps }),
                )?;

                eyre::Ok((Some(tx), result.traces))
            })
            .collect::<Vec<_>>();

        let mut abort = false;
        for res in join_all(futs).await {
            let (tx, traces) = res?;

            // Transaction will be `None`, if execution didn't pass.
            if tx.is_none() || script_config.evm_opts.verbosity > 3 {
                // Identify all contracts created during the call.
                if traces.is_empty() {
                    eyre::bail!(
                        "forge script requires tracing enabled to collect created contracts"
                    );
                }

                for (_, trace) in &traces {
                    println!("{}", render_trace_arena(trace, decoder).await?);
                }
            }

            if let Some(tx) = tx {
                final_txs.push_back(tx);
            } else {
                abort = true;
            }
        }

        if abort {
            eyre::bail!("Simulated execution failed.")
        }

        Ok(final_txs)
    }

    /// Build the multiple runners from different forks.
    async fn build_runners(
        &self,
        script_config: &ScriptConfig,
        dual_compiled_contracts: Option<DualCompiledContracts>,
    ) -> Result<HashMap<RpcUrl, ScriptRunner>> {
        let sender = script_config.evm_opts.sender;

        if !shell::verbosity().is_silent() {
            let n = script_config.total_rpcs.len();
            let s = if n != 1 { "s" } else { "" };
            println!("\n## Setting up {n} EVM{s}.");
        }

        let futs = script_config
            .total_rpcs
            .iter()
            .map(|rpc| async {
                let mut script_config = script_config.clone();
                script_config.evm_opts.fork_url = Some(rpc.clone());
                let runner = self
                    .prepare_runner(
                        &mut script_config,
                        sender,
                        SimulationStage::OnChain,
                        None,
                        dual_compiled_contracts.clone(),
                    )
                    .await?;
                Ok((rpc.clone(), runner))
            })
            .collect::<Vec<_>>();

        join_all(futs).await.into_iter().collect()
    }

    /// Creates the Runner that drives script execution
    async fn prepare_runner(
        &self,
        script_config: &mut ScriptConfig,
        sender: Address,
        stage: SimulationStage,
        script_wallets: Option<ScriptWallets>,
        dual_compiled_contracts: Option<DualCompiledContracts>,
    ) -> Result<ScriptRunner> {
        trace!("preparing script runner");
        let env = script_config.evm_opts.evm_env().await?;

        // The db backend that serves all the data.
        let db = match &script_config.evm_opts.fork_url {
            Some(url) => match script_config.backends.get(url) {
                Some(db) => db.clone(),
                None => {
                    let fork = script_config.evm_opts.get_fork(&script_config.config, env.clone());
                    let backend = Backend::spawn(fork).await;
                    script_config.backends.insert(url.clone(), backend.clone());
                    backend
                }
            },
            None => {
                // It's only really `None`, when we don't pass any `--fork-url`. And if so, there is
                // no need to cache it, since there won't be any onchain simulation that we'd need
                // to cache the backend for.
                Backend::spawn(script_config.evm_opts.get_fork(&script_config.config, env.clone()))
                    .await
            }
        };

        // We need to enable tracing to decode contract names: local or external.
        let mut builder = ExecutorBuilder::new()
            .inspectors(|stack| stack.trace(true))
            .spec(script_config.config.evm_spec_id())
            .gas_limit(script_config.evm_opts.gas_limit());

        let use_zk = script_config.config.zksync.run_in_zk_mode();
        if let SimulationStage::Local = stage {
            builder = builder.inspectors(|stack| {
                stack
                    .debug(self.debug)
                    .cheatcodes(
                        CheatsConfig::new(
                            &script_config.config,
                            script_config.evm_opts.clone(),
                            script_wallets,
                            dual_compiled_contracts.unwrap_or_default(),
                            use_zk,
                        )
                        .into(),
                    )
                    .enable_isolation(script_config.evm_opts.isolate)
            });
        }

        let mut executor = builder.build(env, db);
        executor.use_zk = use_zk;
        Ok(ScriptRunner::new(executor, script_config.evm_opts.initial_balance, sender))
    }
}
