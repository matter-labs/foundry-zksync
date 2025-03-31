//! Contains zksync-specific code to run `forge create`

use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    sync::Arc,
};

use super::{ContractDeploymentError, ContractFactory, CreateArgs, DeploymentTxFactory};
use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
use alloy_json_abi::JsonAbi;
use alloy_network::{Network, ReceiptResponse, TransactionBuilder};
use alloy_primitives::{hex, Address, Bytes};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_signer::Signer;
use alloy_zksync::{
    network::{
        transaction_request::TransactionRequest, unsigned_tx::eip712::PaymasterParams, Zksync,
    },
    wallet::ZksyncWallet,
};
use clap::Parser;
use eyre::{Context, Result};
use forge_verify::VerifyArgs;
use foundry_cli::{
    opts::EtherscanOpts,
    utils,
    utils::{read_constructor_args_file, remove_zk_contract, LoadConfig},
};
use foundry_common::{compile::ProjectCompiler, shell};
use foundry_compilers::{artifacts::BytecodeObject, utils::canonicalize, ArtifactId, Project};
use foundry_zksync_compilers::compilers::artifact_output::zk::ZkContractArtifact;
use foundry_zksync_core::convert::ConvertH160;
use serde_json::json;

#[derive(Clone, Debug, Parser)]
pub struct ZkCreateArgs {
    /// Gas per pubdata
    #[clap(long = "zk-gas-per-pubdata", value_name = "GAS_PER_PUBDATA")]
    pub gas_per_pubdata: Option<u64>,
}

#[derive(Debug, Default)]
/// Data used to deploy a contract on zksync
pub struct ZkSyncData {
    #[allow(dead_code)]
    bytecode: Vec<u8>,
    factory_deps: Vec<Vec<u8>>,
    paymaster_params: Option<PaymasterParams>,
}

impl CreateArgs {
    pub(super) async fn run_zksync(mut self, project: Project) -> Result<()> {
        let paymaster_params = if let Some(paymaster_address) =
            self.build.compiler.zk.paymaster_address
        {
            Some(PaymasterParams {
                paymaster: paymaster_address,
                paymaster_input: self.build.compiler.zk.paymaster_input.clone().unwrap_or_default(),
            })
        } else {
            None
        };
        let target_path = if let Some(ref mut path) = self.contract.path {
            canonicalize(project.root().join(path))?
        } else {
            project.find_contract_path(&self.contract.name)?
        };

        let config = self.build.load_config()?;
        let zk_project =
            foundry_config::zksync::config_create_project(&config, config.cache, false)?;
        let zk_compiler = ProjectCompiler::new().files([target_path.clone()]);
        let mut zk_output = zk_compiler.zksync_compile(&zk_project)?;

        let (artifact, id) = remove_zk_contract(&mut zk_output, &target_path, &self.contract.name)?;

        let ZkContractArtifact { bytecode, abi, factory_dependencies, .. } = &artifact;

        let abi = abi.clone().expect("Abi not found");
        let bin = bytecode.as_ref().expect("Bytecode not found");

        let bytecode = match bin.object() {
            BytecodeObject::Bytecode(bytes) => bytes.to_vec(),
            _ => {
                let link_refs = bin
                    .missing_libraries
                    .iter()
                    .map(|library| {
                        let mut parts = library.split(':');
                        let path = parts.next().unwrap();
                        let name = parts.next().unwrap();
                        format!("\t{name}: {path}")
                    })
                    .collect::<HashSet<String>>()
                    .into_iter()
                    .collect::<Vec<String>>()
                    .join("\n");
                eyre::bail!("Dynamic linking not supported in `create` command - deploy the following library contracts first, then provide the address to link at compile time\n{}", link_refs)
            }
        };

        // Add arguments to constructor
        let config = self.eth.load_config()?;
        let provider = utils::get_provider_zksync(&config)?;
        let params = match abi.constructor {
            Some(ref v) => {
                let constructor_args =
                    if let Some(ref constructor_args_path) = self.constructor_args_path {
                        read_constructor_args_file(constructor_args_path.to_path_buf())?
                    } else {
                        self.constructor_args.clone()
                    };
                self.parse_constructor_args(v, &constructor_args)?
            }
            None => vec![],
        };

        // respect chain, if set explicitly via cmd args
        let chain_id = if let Some(chain_id) = self.chain_id() {
            chain_id
        } else {
            provider.get_chain_id().await?
        };

        let factory_deps: Vec<Vec<u8>> = {
            let factory_dependencies_map =
                factory_dependencies.as_ref().expect("factory deps not found");
            let mut visited_paths = HashSet::new();
            let mut visited_bytecodes = HashSet::new();
            let mut queue = VecDeque::new();

            for dep in factory_dependencies_map.values() {
                queue.push_back(dep.clone());
            }

            while let Some(dep_info) = queue.pop_front() {
                if visited_paths.insert(dep_info.clone()) {
                    let mut split = dep_info.split(':');
                    let contract_path = split
                        .next()
                        .expect("Failed to extract contract path for factory dependency");
                    let contract_name = split
                        .next()
                        .expect("Failed to extract contract name for factory dependency");
                    let mut abs_path_buf = PathBuf::new();
                    abs_path_buf.push(project.root());
                    abs_path_buf.push(contract_path);
                    let fdep_art =
                            zk_output.find(&abs_path_buf, contract_name).unwrap_or_else(|| {
                                panic!(
                                    "Could not find contract {contract_name} at path {contract_path} for compilation output",
                                )
                            });
                    let fdep_fdeps_map =
                        fdep_art.factory_dependencies.as_ref().expect("factory deps not found");
                    for dep in fdep_fdeps_map.values() {
                        queue.push_back(dep.clone())
                    }

                    // NOTE(zk): unlinked factory deps don't show up in `factory_dependencies`
                    let fdep_bytecode = fdep_art
                        .bytecode
                        .clone()
                        .expect("Bytecode not found for factory dependency")
                        .object()
                        .into_bytes()
                        .unwrap()
                        .to_vec();
                    visited_bytecodes.insert(fdep_bytecode);
                }
            }
            visited_bytecodes.insert(bytecode.clone());
            visited_bytecodes.into_iter().collect()
        };
        let zk_data = ZkSyncData { bytecode, factory_deps, paymaster_params };

        if self.unlocked {
            // Deploy with unlocked account
            let sender = self.eth.wallet.from.expect("required");
            self.deploy_zk(
                abi,
                bin.object(),
                params,
                provider,
                chain_id,
                sender,
                config.transaction_timeout,
                id,
                zk_data,
            )
            .await
        } else {
            // Deploy with signer
            // Avoid initializing `signer` twice as it will error out with Ledger
            // and potentially other devices that rely on HID too
            let zk_signer = self.eth.wallet.signer().await?;
            let deployer = zk_signer.address();
            let provider = ProviderBuilder::<_, _, Zksync>::default()
                .wallet(ZksyncWallet::new(zk_signer))
                .on_provider(provider);
            self.deploy_zk(
                abi,
                bin.object(),
                params,
                provider,
                chain_id,
                deployer,
                config.transaction_timeout,
                id,
                zk_data,
            )
            .await
        }
    }

    /// Deploys the contract using ZKsync provider.
    #[allow(clippy::too_many_arguments)]
    async fn deploy_zk<P: Provider<Zksync>>(
        self,
        abi: JsonAbi,
        bin: BytecodeObject,
        args: Vec<DynSolValue>,
        provider: P,
        chain: u64,
        deployer_address: Address,
        timeout: u64,
        id: ArtifactId,
        zk_data: ZkSyncData,
    ) -> Result<()> {
        let bin = bin.into_bytes().unwrap_or_else(|| {
            panic!("no bytecode found in bin object for {}", self.contract.name)
        });
        let provider = Arc::new(provider);
        let factory = ContractFactory::new_zk(abi.clone(), bin.clone(), provider.clone(), timeout);

        let is_args_empty = args.is_empty();
        let mut deployer =
            factory.deploy_tokens_zk(args.clone(), &zk_data).context("failed to deploy contract").map_err(|e| {
                if is_args_empty {
                    e.wrap_err("no arguments provided for contract constructor; consider --constructor-args or --constructor-args-path")
                } else {
                    e
                }
            })?;

        deployer.tx = deployer.tx.with_factory_deps(
            zk_data.factory_deps.clone().into_iter().map(|dep| dep.into()).collect(),
        );
        if let Some(paymaster_params) = zk_data.paymaster_params {
            deployer.tx.set_paymaster_params(paymaster_params);
        }
        deployer.tx.set_from(deployer_address);
        deployer.tx.set_chain_id(chain);
        // `to` field must be set explicitly, cannot be None.
        if deployer.tx.to().is_none() {
            deployer.tx.set_create();
        }
        deployer.tx.set_nonce(if let Some(nonce) = self.tx.nonce {
            Ok(nonce.to())
        } else {
            provider.get_transaction_count(deployer_address).await
        }?);

        // set tx value if specified
        if let Some(value) = self.tx.value {
            deployer.tx.set_value(value);
        }

        let gas_price = if let Some(gas_price) = self.tx.gas_price {
            gas_price.to()
        } else {
            provider.get_gas_price().await?
        };
        deployer.tx.set_gas_price(gas_price);

        // estimate fee
        foundry_zksync_core::estimate_fee(
            &mut deployer.tx,
            &provider,
            130,
            self.zksync.gas_per_pubdata,
        )
        .await?;

        if let Some(gas_limit) = self.tx.gas_limit {
            deployer.tx.set_gas_limit(gas_limit.to::<u64>());
        };

        // Before we actually deploy the contract we try check if the verify settings are valid
        let mut constructor_args = None;
        if self.verify {
            if !args.is_empty() {
                let encoded_args = abi
                    .constructor()
                    .ok_or_else(|| eyre::eyre!("could not find constructor"))?
                    .abi_encode_input(&args)?;
                constructor_args = Some(hex::encode_prefixed(encoded_args));
            }

            self.verify_preflight_check(constructor_args.clone(), chain, &id).await?;
        }

        // Deploy the actual contract
        let (deployed_contract, receipt) = deployer.send_with_receipt().await?;
        let tx_hash = receipt.transaction_hash();

        let address = deployed_contract;
        if shell::is_json() {
            let output = json!({
                "deployer": deployer_address.to_string(),
                "deployedTo": address.to_string(),
                "transactionHash": tx_hash
            });
            sh_println!("{output}")?;
        } else {
            sh_println!("Deployer: {deployer_address}")?;
            sh_println!("Deployed to: {address}")?;
            sh_println!("Transaction hash: {:?}", tx_hash)?;
        };

        if !self.verify {
            return Ok(());
        }

        sh_println!("Starting contract verification...")?;

        let num_of_optimizations = if self.build.compiler.optimize.unwrap_or_default() {
            self.build.compiler.optimizer_runs
        } else {
            None
        };
        let verify = VerifyArgs {
            address,
            contract: Some(self.contract),
            compiler_version: None,
            constructor_args,
            constructor_args_path: None,
            num_of_optimizations,
            etherscan: EtherscanOpts { key: self.eth.etherscan.key(), chain: Some(chain.into()) },
            rpc: Default::default(),
            flatten: false,
            force: false,
            skip_is_verified_check: true,
            watch: true,
            retry: self.retry,
            libraries: self.build.libraries.clone(),
            root: None,
            verifier: self.verifier,
            via_ir: self.build.via_ir,
            evm_version: self.build.compiler.evm_version,
            show_standard_json_input: self.show_standard_json_input,
            guess_constructor_args: false,
            compilation_profile: None, //TODO(zk): provide comp profile
            zksync: self.build.compiler.zk.enabled(),
        };
        sh_println!("Waiting for {} to detect contract deployment...", verify.verifier.verifier)?;
        verify.run().await
    }
}

/// Helper which manages the deployment transaction of a smart contract
#[derive(Debug)]
#[must_use = "Deployer does nothing unless you `send` it"]
pub struct ZkDeployer<P> {
    /// The deployer's transaction, exposed for overriding the defaults
    pub tx: TransactionRequest,
    abi: JsonAbi,
    client: P,
    confs: usize,
    timeout: u64,
    zk_factory_deps: Option<Vec<Vec<u8>>>,
}

impl<P> Clone for ZkDeployer<P>
where
    P: Clone + Provider<Zksync>,
{
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            abi: self.abi.clone(),
            client: self.client.clone(),
            confs: self.confs,
            timeout: self.timeout,
            zk_factory_deps: self.zk_factory_deps.clone(),
        }
    }
}

impl<P> ZkDeployer<P>
where
    P: Clone + Provider<Zksync>,
{
    /// Broadcasts the contract deployment transaction and after waiting for it to
    /// be sufficiently confirmed (default: 1), it returns a tuple with
    /// the [`Contract`](crate::Contract) struct at the deployed contract's address
    /// and the corresponding [`AnyReceipt`].
    pub async fn send_with_receipt(
        self,
    ) -> Result<(Address, <Zksync as Network>::ReceiptResponse), ContractDeploymentError> {
        let receipt = self
            .client
            .send_transaction(self.tx)
            .await?
            .with_required_confirmations(self.confs as u64)
            .with_timeout(Some(std::time::Duration::from_secs(self.timeout)))
            .get_receipt()
            .await?;

        let address =
            receipt.contract_address().ok_or(ContractDeploymentError::ContractNotDeployed)?;

        Ok((address, receipt))
    }
}

impl<P> DeploymentTxFactory<P>
where
    P: Provider<Zksync> + Clone,
{
    /// Creates a factory for deployment of the Contract with bytecode, and the
    /// constructor defined in the abi. The client will be used to send any deployment
    /// transaction.
    pub fn new_zk(abi: JsonAbi, bytecode: Bytes, client: P, timeout: u64) -> Self {
        Self { client, abi, bytecode, timeout }
    }

    /// Create a deployment tx using the provided tokens as constructor
    /// arguments
    pub fn deploy_tokens_zk(
        self,
        params: Vec<DynSolValue>,
        zk_data: &ZkSyncData,
    ) -> Result<ZkDeployer<P>, ContractDeploymentError> {
        // Encode the constructor args & concatenate with the bytecode if necessary
        if self.abi.constructor().is_none() && !params.is_empty() {
            return Err(ContractDeploymentError::ConstructorError)
        }

        // Encode the constructor args & concatenate with the bytecode if necessary
        let constructor_args = match self.abi.constructor() {
            None => Default::default(),
            Some(constructor) => constructor.abi_encode_input(&params).unwrap_or_default(),
        };

        let tx = TransactionRequest::default()
            .with_to(foundry_zksync_core::CONTRACT_DEPLOYER_ADDRESS.to_address())
            .with_create_params(
                zk_data.bytecode.clone(),
                constructor_args,
                zk_data.factory_deps.clone(),
            )
            .map_err(|_| ContractDeploymentError::TransactionBuildError)?;

        Ok(ZkDeployer {
            client: self.client.clone(),
            abi: self.abi,
            tx,
            confs: 1,
            timeout: self.timeout,
            zk_factory_deps: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::utils::get_provider_zksync;
    use alloy_zksync::network::tx_type::TxType;

    #[test]
    fn test_zk_deployer_builds_eip712_transactions() {
        let client = get_provider_zksync(&Default::default()).expect("failed creating client");
        let factory =
            DeploymentTxFactory::new_zk(Default::default(), Default::default(), client, 0);

        let deployer = factory
            .deploy_tokens_zk(
                Default::default(),
                &ZkSyncData { bytecode: [0u8; 32].into(), ..Default::default() },
            )
            .expect("failed deploying tokens");

        assert_eq!(TxType::Eip712, deployer.tx.output_tx_type());
    }
}
