use alloy_consensus::Transaction;
use alloy_network::{AnyNetwork, TransactionResponse};
use alloy_primitives::{
    Address, Bytes,
    map::{HashMap, HashSet},
};
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_types::BlockTransactions;
use alloy_serde::OtherFields;
use clap::Parser;
use eyre::{Result, WrapErr};
use foundry_cli::{
    opts::{EtherscanOpts, RpcOpts},
    utils::{self, TraceResult, handle_traces, init_progress},
};
use foundry_common::{SYSTEM_TRANSACTION_TYPE, is_known_system_sender, shell};
use foundry_compilers::artifacts::EvmVersion;
use foundry_config::{
    Config,
    figment::{
        self, Figment, Metadata, Profile,
        value::{Dict, Map},
    },
};
use foundry_evm::{
    Env,
    executors::{EvmError, TracingExecutor},
    opts::EvmOpts,
    traces::{InternalTraceMode, TraceMode, Traces},
    utils::configure_tx_env,
};
use foundry_evm_core::env::AsEnvMut;

use crate::utils::apply_chain_and_block_specific_env_changes;
use zksync_types::Transaction as ZkTransaction;

/// CLI arguments for `cast run`.
#[derive(Clone, Debug, Parser)]
pub struct RunArgs {
    /// The transaction hash.
    tx_hash: String,

    /// Opens the transaction in the debugger.
    #[arg(long, short)]
    debug: bool,

    /// Whether to identify internal functions in traces.
    #[arg(long)]
    decode_internal: bool,

    /// Print out opcode traces.
    #[arg(long, short)]
    trace_printer: bool,

    /// Executes the transaction only with the state from the previous block.
    ///
    /// May result in different results than the live execution!
    #[arg(long)]
    quick: bool,

    /// Disables the labels in the traces.
    #[arg(long, default_value_t = false)]
    disable_labels: bool,

    /// Label addresses in the trace.
    ///
    /// Example: 0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045:vitalik.eth
    #[arg(long, short)]
    label: Vec<String>,

    #[command(flatten)]
    etherscan: EtherscanOpts,

    #[command(flatten)]
    rpc: RpcOpts,

    /// The EVM version to use.
    ///
    /// Overrides the version specified in the config.
    #[arg(long)]
    evm_version: Option<EvmVersion>,

    /// Sets the number of assumed available compute units per second for this provider
    ///
    /// default value: 330
    ///
    /// See also, <https://docs.alchemy.com/reference/compute-units#what-are-cups-compute-units-per-second>
    #[arg(long, alias = "cups", value_name = "CUPS")]
    pub compute_units_per_second: Option<u64>,

    /// Disables rate limiting for this node's provider.
    ///
    /// default value: false
    ///
    /// See also, <https://docs.alchemy.com/reference/compute-units#what-are-cups-compute-units-per-second>
    #[arg(long, value_name = "NO_RATE_LIMITS", visible_alias = "no-rpc-rate-limit")]
    pub no_rate_limit: bool,

    /// Enables Odyssey features.
    #[arg(long, alias = "alphanet")]
    pub odyssey: bool,

    /// Use current project artifacts for trace decoding.
    #[arg(long, visible_alias = "la")]
    pub with_local_artifacts: bool,

    /// Disable block gas limit check.
    #[arg(long)]
    pub disable_block_gas_limit: bool,

    /// Run a ZKsync transaction.
    #[arg(long = "zksync")]
    zk_force: bool,

    /// Disables storage caching entirely.
    /// NOTE(zk) This is needed so tests don't cache anvil-zksync responses, could also be useful
    /// upstream.
    #[arg(long)]
    no_storage_caching: bool,
}

impl RunArgs {
    /// Executes the transaction by replaying it
    ///
    /// This replays the entire block the transaction was mined in unless `quick` is set to true
    ///
    /// Note: This executes the transaction(s) as is: Cheatcodes are disabled
    pub async fn run(self) -> Result<()> {
        let figment = Into::<Figment>::into(&self.rpc).merge(&self);
        let evm_opts = figment.extract::<EvmOpts>()?;
        let mut config = Config::from_provider(figment)?.sanitized();
        config.zksync.compile = self.zk_force;
        config.no_storage_caching = self.no_storage_caching;
        let strategy = utils::get_executor_strategy(&config);

        let compute_units_per_second =
            if self.no_rate_limit { Some(u64::MAX) } else { self.compute_units_per_second };

        let provider = foundry_cli::utils::get_provider_builder(&config)?
            .compute_units_per_second_opt(compute_units_per_second)
            .build()?;

        let tx_hash = self.tx_hash.parse().wrap_err("invalid tx hash")?;
        let tx = provider
            .get_transaction_by_hash(tx_hash)
            .await
            .wrap_err_with(|| format!("tx not found: {tx_hash:?}"))?
            .ok_or_else(|| eyre::eyre!("tx not found: {:?}", tx_hash))?;

        // check if the tx is a system transaction
        if is_known_system_sender(tx.from())
            || tx.transaction_type() == Some(SYSTEM_TRANSACTION_TYPE)
        {
            return Err(eyre::eyre!(
                "{:?} is a system transaction.\nReplaying system transactions is currently not supported.",
                tx.tx_hash()
            ));
        }

        let tx_block_number =
            tx.block_number.ok_or_else(|| eyre::eyre!("tx may still be pending: {:?}", tx_hash))?;

        // fetch the block the transaction was mined in
        let block = provider.get_block(tx_block_number.into()).full().await?;

        // Only fetch raw block data if zk_force is enabled
        let mut raw_block = None;
        if self.zk_force {
            raw_block = Some(
                provider
                    .raw_request::<_, Vec<ZkTransaction>>(
                        "zks_getRawBlockTransactions".into(),
                        vec![serde_json::json!(tx_block_number)],
                    )
                    .await?,
            );
        }

        // we need to fork off the parent block
        config.fork_block_number = Some(tx_block_number - 1);

        let create2_deployer = evm_opts.create2_deployer;
        let (mut env, fork, chain, odyssey) =
            TracingExecutor::get_fork_material(&config, evm_opts).await?;
        let mut evm_version = self.evm_version;

        env.evm_env.cfg_env.disable_block_gas_limit = self.disable_block_gas_limit;
        env.evm_env.block_env.number = tx_block_number;

        if let Some(block) = &block {
            env.evm_env.block_env.timestamp = block.header.timestamp;
            env.evm_env.block_env.beneficiary = block.header.beneficiary;
            env.evm_env.block_env.difficulty = block.header.difficulty;
            env.evm_env.block_env.prevrandao = Some(block.header.mix_hash.unwrap_or_default());
            env.evm_env.block_env.basefee = block.header.base_fee_per_gas.unwrap_or_default();
            env.evm_env.block_env.gas_limit = block.header.gas_limit;

            // TODO: we need a smarter way to map the block to the corresponding evm_version for
            // commonly used chains
            if evm_version.is_none() {
                // if the block has the excess_blob_gas field, we assume it's a Cancun block
                if block.header.excess_blob_gas.is_some() {
                    evm_version = Some(EvmVersion::Prague);
                }
            }
            apply_chain_and_block_specific_env_changes::<AnyNetwork>(env.as_env_mut(), block);
        }

        let trace_mode = TraceMode::Call
            .with_debug(self.debug)
            .with_decode_internal(if self.decode_internal {
                InternalTraceMode::Full
            } else {
                InternalTraceMode::None
            })
            .with_state_changes(shell::verbosity() > 4);
        let mut executor = TracingExecutor::new(
            env.clone(),
            fork,
            evm_version,
            trace_mode,
            odyssey,
            create2_deployer,
            None,
            strategy,
        )?;
        let mut env = Env::new_with_spec_id(
            env.evm_env.cfg_env.clone(),
            env.evm_env.block_env.clone(),
            env.tx.clone(),
            executor.spec_id(),
        );

        if self.zk_force {
            // Set up fee parameters.
            executor.strategy.runner.zksync_set_fork_env(
                executor.strategy.context.as_mut(),
                &config.get_rpc_url_or_localhost_http()?,
                &env,
            )?;
        }

        // Set the state to the moment right before the transaction
        if !self.quick {
            if !shell::is_json() {
                sh_println!("Executing previous transactions from the block.")?;
            }

            if let Some(block) = block {
                let pb = init_progress(block.transactions.len() as u64, "tx");
                pb.set_position(0);

                let BlockTransactions::Full(ref txs) = block.transactions else {
                    return Err(eyre::eyre!("Could not get block txs"));
                };

                for (index, tx) in txs.iter().enumerate() {
                    // System transactions such as on L2s don't contain any pricing info so
                    // we skip them otherwise this would cause
                    // reverts
                    if is_known_system_sender(tx.from())
                        || tx.transaction_type() == Some(SYSTEM_TRANSACTION_TYPE)
                    {
                        pb.set_position((index + 1) as u64);
                        continue;
                    }
                    if tx.tx_hash() == tx_hash {
                        break;
                    }

                    if self.zk_force {
                        let raw_tx = &raw_block
                            .as_ref()
                            .unwrap()
                            .get(index)
                            .expect("Failed to get transaction");
                        let metadata = configure_zksync_tx_env(&mut env, raw_tx);
                        let mut other_fields = OtherFields::default();
                        other_fields.insert(
                            foundry_zksync_core::ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY.to_string(),
                            serde_json::to_value(metadata)
                                .expect("Failed to serialize ZkTransactionMetadata"),
                        );

                        executor.strategy.runner.zksync_set_transaction_context(
                            executor.strategy.context.as_mut(),
                            other_fields,
                        );
                    } else {
                        configure_tx_env(&mut env.as_env_mut(), &tx.inner);
                    }

                    if let Some(to) = Transaction::to(tx) {
                        trace!(tx=?tx.tx_hash(),?to, "executing previous call transaction");
                        executor.transact_with_env(env.clone()).wrap_err_with(|| {
                            format!(
                                "Failed to execute transaction: {:?} in block {}",
                                tx.tx_hash(),
                                env.evm_env.block_env.number
                            )
                        })?;
                    } else {
                        trace!(tx=?tx.tx_hash(), "executing previous create transaction");
                        if let Err(error) = executor.deploy_with_env(env.clone(), None) {
                            match error {
                                // Reverted transactions should be skipped
                                EvmError::Execution(_) => (),
                                error => {
                                    return Err(error).wrap_err_with(|| {
                                        format!(
                                            "Failed to deploy transaction: {:?} in block {}",
                                            tx.tx_hash(),
                                            env.evm_env.block_env.number
                                        )
                                    });
                                }
                            }
                        }
                    }

                    pb.set_position((index + 1) as u64);
                }
            }
        }

        // Execute our transaction
        let result = {
            executor.set_trace_printer(self.trace_printer);

            if self.zk_force {
                let raw_txs = raw_block
                    .as_ref()
                    .ok_or_else(|| eyre::eyre!("Raw block data not available"))?;

                // Find the raw transaction that matches our target transaction hash
                let raw_tx = raw_txs
                    .iter()
                    .find(|raw_tx| raw_tx.hash() == zksync_types::H256::from(tx.tx_hash().0))
                    .ok_or_else(|| eyre::eyre!("Could not find target transaction in raw block"))?;

                let metadata = configure_zksync_tx_env(&mut env, raw_tx);
                let mut other_fields = OtherFields::default();
                other_fields.insert(
                    foundry_zksync_core::ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY.to_string(),
                    serde_json::to_value(metadata)
                        .expect("Failed to serialize ZkTransactionMetadata"),
                );

                executor.strategy.runner.zksync_set_transaction_context(
                    executor.strategy.context.as_mut(),
                    other_fields,
                );
            } else {
                configure_tx_env(&mut env.as_env_mut(), &tx.inner);
            }

            if let Some(to) = Transaction::to(&tx) {
                trace!(tx=?tx.tx_hash(), to=?to, "executing call transaction");
                TraceResult::try_from(executor.transact_with_env(env))?
            } else {
                trace!(tx=?tx.tx_hash(), "executing create transaction");
                TraceResult::try_from(executor.deploy_with_env(env, None))?
            }
        };

        let contracts_bytecode = fetch_contracts_bytecode_from_trace(&provider, &result).await?;
        handle_traces(
            result,
            &config,
            chain,
            &contracts_bytecode,
            self.label,
            self.with_local_artifacts,
            self.debug,
            self.decode_internal,
            self.disable_labels,
        )
        .await?;

        Ok(())
    }
}

pub async fn fetch_contracts_bytecode_from_trace(
    provider: &RootProvider<AnyNetwork>,
    result: &TraceResult,
) -> Result<HashMap<Address, Bytes>> {
    let mut contracts_bytecode = HashMap::default();
    if let Some(ref traces) = result.traces {
        let addresses = gather_trace_addresses(traces);
        let results = futures::future::join_all(addresses.into_iter().map(async |a| {
            (
                a,
                provider.get_code_at(a).await.unwrap_or_else(|e| {
                    sh_warn!("Failed to fetch code for {a:?}: {e:?}").ok();
                    Bytes::new()
                }),
            )
        }))
        .await;
        for (address, code) in results {
            if !code.is_empty() {
                contracts_bytecode.insert(address, code);
            }
        }
    }
    Ok(contracts_bytecode)
}

fn gather_trace_addresses(traces: &Traces) -> HashSet<Address> {
    let mut addresses = HashSet::default();
    for (_, trace) in traces {
        for node in trace.arena.nodes() {
            if !node.trace.address.is_zero() {
                addresses.insert(node.trace.address);
            }
            if !node.trace.caller.is_zero() {
                addresses.insert(node.trace.caller);
            }
        }
    }
    addresses
}

impl figment::Provider for RunArgs {
    fn metadata(&self) -> Metadata {
        Metadata::named("RunArgs")
    }

    fn data(&self) -> Result<Map<Profile, Dict>, figment::Error> {
        let mut map = Map::new();

        if self.odyssey {
            map.insert("odyssey".into(), self.odyssey.into());
        }

        if let Some(api_key) = &self.etherscan.key {
            map.insert("etherscan_api_key".into(), api_key.as_str().into());
        }

        if let Some(api_version) = &self.etherscan.api_version {
            map.insert("etherscan_api_version".into(), api_version.to_string().into());
        }

        if let Some(evm_version) = self.evm_version {
            map.insert("evm_version".into(), figment::value::Value::serialize(evm_version)?);
        }

        Ok(Map::from([(Config::selected_profile(), map)]))
    }
}

/// Configures the env for the given RPC transaction.
/// Accounts for an impersonated transaction by resetting the `env.tx.caller` field to `tx.from`.
pub fn configure_zksync_tx_env(
    env: &mut Env,
    outer_tx: &ZkTransaction,
) -> foundry_zksync_core::ZkTransactionMetadata {
    use alloy_primitives::TxKind;
    use foundry_zksync_core::convert::{ConvertH160, ConvertU256};
    use zksync_types::ExecuteTransactionCommon;

    // Extract fields from the raw zkSync transaction format

    // Set basic transaction fields
    env.tx.caller = outer_tx.initiator_account().to_address();

    env.tx.kind = TxKind::Call(
        outer_tx.recipient_account().expect("recipient_account not found in execute").to_address(),
    );
    env.tx.gas_limit = match &outer_tx.common_data {
        ExecuteTransactionCommon::L2(l2) => l2.fee.gas_limit.as_u64(),
        _ => outer_tx.gas_limit().as_u64(),
    };
    env.tx.nonce = outer_tx.nonce().expect("nonce not found in common_data").0.into();

    env.tx.value = outer_tx.execute.value.to_ru256();

    env.tx.data = outer_tx.execute.calldata.clone().into();
    env.tx.gas_price = outer_tx.max_fee_per_gas().low_u128();

    match &outer_tx.common_data {
        // Set zkSync specific metadata
        ExecuteTransactionCommon::L2(common_data) => foundry_zksync_core::ZkTransactionMetadata {
            factory_deps: outer_tx.execute.factory_deps.clone(),
            paymaster_data: Some(common_data.paymaster_params.clone()),
        },
        _ => foundry_zksync_core::ZkTransactionMetadata {
            factory_deps: outer_tx.execute.factory_deps.clone(),
            paymaster_data: None,
        },
    }
}
