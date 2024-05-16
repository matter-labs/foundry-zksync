use super::{retry::RetryArgs, verify};

use alloy_dyn_abi::{DynSolValue, JsonAbiExt, ResolveSolType};
use alloy_json_abi::{Constructor, JsonAbi};
use alloy_primitives::{Address, Bytes};

use clap::{Parser, ValueHint};
use ethers_contract::ContractError;
use ethers_core::{
    abi::InvalidOutputType,
    types::{
        transaction::eip2718::TypedTransaction, BlockNumber, Eip1559TransactionRequest,
        NameOrAddress, TransactionReceipt, TransactionRequest,
    },
};
use ethers_middleware::SignerMiddleware;
use ethers_providers::Middleware;
use eyre::{Context, Result};
use foundry_cli::{
    opts::{CoreBuildArgs, EthereumOpts, EtherscanOpts, TransactionOpts},
    utils::{self, read_constructor_args_file, remove_contract, LoadConfig},
};
use foundry_common::{
    compile::ProjectCompiler,
    fmt::parse_tokens,
    provider::ethers::estimate_eip1559_fees,
    types::{ToAlloy, ToEthers},
};
use foundry_compilers::{
    artifacts::{BytecodeObject, CompactBytecode},
    info::ContractInfo,
    utils::canonicalized,
};
use foundry_config::Chain;
use foundry_wallets::WalletSigner;
use foundry_zksync_compiler::{libraries as zklibs, DualCompiledContract, DualCompiledContracts};

use serde_json::json;
use std::{borrow::Borrow, marker::PhantomData, path::PathBuf, sync::Arc};

/// CLI arguments for `forge create`.
#[derive(Clone, Debug, Parser)]
pub struct CreateArgs {
    /// The contract identifier in the form `<path>:<contractname>`.
    contract: Option<ContractInfo>,

    /// The constructor arguments.
    #[clap(
        long,
        num_args(1..),
        conflicts_with = "constructor_args_path",
        value_name = "ARGS",
    )]
    constructor_args: Vec<String>,

    /// The path to a file containing the constructor arguments.
    #[clap(
        long,
        value_hint = ValueHint::FilePath,
        value_name = "PATH",
    )]
    constructor_args_path: Option<PathBuf>,

    /// Deploy the missing dependency libraries from last build.
    #[clap(
        long,
        help = "Deploy the missing dependency libraries from last build.",
        default_value_t = false
    )]
    deploy_missing_libraries: bool,

    /// Print the deployment information as JSON.
    #[clap(long, help_heading = "Display options")]
    json: bool,

    /// Verify contract after creation.
    #[clap(long)]
    verify: bool,

    /// Send via `eth_sendTransaction` using the `--from` argument or `$ETH_FROM` as sender
    #[clap(long, requires = "from")]
    unlocked: bool,

    /// Prints the standard json compiler input if `--verify` is provided.
    ///
    /// The standard json compiler input can be used to manually submit contract verification in
    /// the browser.
    #[clap(long, requires = "verify")]
    show_standard_json_input: bool,

    #[clap(flatten)]
    opts: CoreBuildArgs,

    #[clap(flatten)]
    tx: TransactionOpts,

    #[clap(flatten)]
    eth: EthereumOpts,

    #[clap(flatten)]
    pub verifier: verify::VerifierArgs,

    #[clap(flatten)]
    retry: RetryArgs,

    #[clap(long)]
    factory_deps: Vec<ContractInfo>,
}

impl CreateArgs {
    /// Executes the command to create a contract
    pub async fn run(mut self) -> Result<()> {
        let mut config = self.eth.try_load_config_emit_warnings()?;
        let project_root = config.project_paths().root;
        let zksync = self.opts.compiler.zksync;

        // Resolve missing libraries
        let libs_batches = if zksync && self.deploy_missing_libraries {
            let missing_libraries = zklibs::get_detected_missing_libraries(&project_root)?;

            let mut all_deployed_libraries = Vec::with_capacity(config.libraries.len());
            for library in &config.libraries {
                let split_lib = library.split(':').collect::<Vec<&str>>();
                let lib_path = split_lib[0];
                let lib_name = split_lib[1];
                all_deployed_libraries.push(ContractInfo {
                    name: lib_name.to_string(),
                    path: Some(lib_path.to_string()),
                });
            }
            info!("Resolving missing libraries");

            zklibs::resolve_libraries(missing_libraries, &all_deployed_libraries)?
        } else {
            vec![]
        };

        let deploying_libraries = !libs_batches.is_empty();
        let contracts_to_deploy = if !deploying_libraries {
            vec![vec![self
                .contract
                .clone()
                .ok_or_else(|| eyre::eyre!("Contract to deploy must be passed"))?]]
        } else {
            libs_batches
        };

        //let mut input_contracts_to_compile = self.opts.compiler.contracts_to_compile.clone();
        for contracts_batch in contracts_to_deploy {
            // Find Project & Compile
            let project = self.opts.project()?;
            let mut output =
                ProjectCompiler::new().quiet_if(self.json || self.opts.silent).compile(&project)?;

            /* TODO: see if we need to support this
            let contracts_to_compile = if !deploying_libraries {
                self.opts.compiler.contracts_to_compile.clone()
            } else {
                Some(
                    contracts_batch
                        .iter()
                        .map(|lib| lib.path.clone().expect("libraries must specify path"))
                        .map(|path| {
                            PathBuf::from(path)
                                .file_name()
                                .expect("contract path to have filename")
                                .to_string_lossy()
                                .to_string()
                        })
                        // respect passed in --contracts-to-compile but don't deploy them
                        .chain(input_contracts_to_compile.take().into_iter().flatten())
                        .collect(),
                )
            };
            */

            let zk_compiler = ProjectCompiler::new().quiet_if(self.json || self.opts.silent);
            let zk_output = zk_compiler.zksync_compile(&project)?;
            let dual_compiled_contracts = DualCompiledContracts::new(&output, &zk_output);

            for mut contract in contracts_batch {
                if let Some(ref mut path) = contract.path {
                    // paths are absolute in the project's output
                    *path = canonicalized(project.root().join(&path)).to_string_lossy().to_string();
                }

                let (abi, bin, _) = remove_contract(&mut output, &contract)?;

                let (bin, zk_data) = if zksync {
                    let contract = bin
                        .object
                        .as_bytes()
                        .and_then(|bytes| dual_compiled_contracts.find_by_evm_bytecode(&bytes.0))
                        .ok_or(eyre::eyre!(
                            "Could not find zksolc contract for contract {}",
                            contract.name
                        ))?;

                    let zk_bin = CompactBytecode {
                        object: BytecodeObject::Bytecode(Bytes::from(
                            contract.zk_deployed_bytecode.clone(),
                        )),
                        link_references: Default::default(),
                        source_map: Default::default(),
                    };

                    let mut factory_deps = dual_compiled_contracts.fetch_all_factory_deps(contract);

                    // for manual specified factory deps
                    for mut contract in std::mem::take(&mut self.factory_deps) {
                        if let Some(path) = contract.path.as_mut() {
                            *path = canonicalized(project.root().join(&path))
                                .to_string_lossy()
                                .to_string();
                        }

                        let (_, bin, _) =
                            remove_contract(&mut output, &contract).with_context(|| {
                                format!(
                                    "Unable to find specified factory deps ({}) in project",
                                    contract.name
                                )
                            })?;

                        let zk = bin
                            .object
                            .as_bytes()
                            .and_then(|bytes| {
                                dual_compiled_contracts.find_by_evm_bytecode(&bytes.0)
                            })
                            .ok_or(eyre::eyre!(
                                "Could not find zksolc contract for contract {}",
                                contract.name
                            ))?;

                        // if the dep isn't already present,
                        // fetch all deps and add them to the final list
                        if !factory_deps.contains(&zk.zk_deployed_bytecode) {
                            let additional_factory_deps =
                                dual_compiled_contracts.fetch_all_factory_deps(zk);
                            factory_deps.extend(additional_factory_deps);
                            factory_deps.dedup();
                        }
                    }

                    (
                        zk_bin,
                        Some((contract, factory_deps.into_iter().map(|bc| bc.to_vec()).collect())),
                    )
                } else {
                    (bin, None)
                };

                let bin = match bin.object {
                    BytecodeObject::Bytecode(_) => bin.object,
                    _ => {
                        let link_refs = bin
                            .link_references
                            .iter()
                            .flat_map(|(path, names)| {
                                names.keys().map(move |name| format!("\t{name}: {path}"))
                            })
                            .collect::<Vec<String>>()
                            .join("\n");
                        eyre::bail!("Dynamic linking not supported in `create` command - deploy the following library contracts first, then provide the address to link at compile time\n{}", link_refs)
                    }
                };

                // Add arguments to constructor
                let provider = utils::get_provider(&config)?;
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
                    provider.get_chainid().await?.as_u64()
                };
                let address = if self.unlocked {
                    // Deploy with unlocked account
                    let sender = self.eth.wallet.from.expect("required");
                    let provider = provider.with_sender(sender.to_ethers());
                    self.deploy(&contract, abi, bin, params, provider, chain_id, zk_data, None)
                        .await?
                } else {
                    // Deploy with signer
                    let signer = self.eth.wallet.signer().await?;
                    let zk_signer = self.eth.wallet.signer().await?;
                    let provider =
                        SignerMiddleware::new_with_provider_chain(provider, signer).await?;
                    self.deploy(
                        &contract,
                        abi,
                        bin,
                        params,
                        provider,
                        chain_id,
                        zk_data,
                        Some(zk_signer),
                    )
                    .await?
                };

                if deploying_libraries {
                    config.libraries.push(format!(
                        "{}:{}:{:#02x}",
                        contract.path.expect("library must have path"),
                        contract.name,
                        address
                    ));
                    config.update_libraries()?;
                }
            }
        }

        if deploying_libraries {
            zklibs::cleanup_detected_missing_libraries(&project_root)?;
        }

        Ok(())
    }

    /// Returns the provided chain id, if any.
    fn chain_id(&self) -> Option<u64> {
        self.eth.etherscan.chain.map(|chain| chain.id())
    }

    /// Ensures the verify command can be executed.
    ///
    /// This is supposed to check any things that might go wrong when preparing a verify request
    /// before the contract is deployed. This should prevent situations where a contract is deployed
    /// successfully, but we fail to prepare a verify request which would require manual
    /// verification.
    async fn verify_preflight_check(
        &self,
        contract: &ContractInfo,
        constructor_args: Option<String>,
        chain: u64,
    ) -> Result<()> {
        // NOTE: this does not represent the same `VerifyArgs` that would be sent after deployment,
        // since we don't know the address yet.
        let mut verify = verify::VerifyArgs {
            address: Default::default(),
            contract: contract.clone(),
            compiler_version: None,
            constructor_args,
            constructor_args_path: None,
            num_of_optimizations: None,
            etherscan: EtherscanOpts { key: self.eth.etherscan.key(), chain: Some(chain.into()) },
            flatten: false,
            force: false,
            skip_is_verified_check: true,
            watch: true,
            retry: self.retry,
            libraries: vec![],
            root: None,
            verifier: self.verifier.clone(),
            via_ir: self.opts.via_ir,
            evm_version: self.opts.compiler.evm_version,
            show_standard_json_input: self.show_standard_json_input,
        };

        // Check config for Etherscan API Keys to avoid preflight check failing if no
        // ETHERSCAN_API_KEY value set.
        let config = verify.load_config_emit_warnings();
        verify.etherscan.key =
            config.get_etherscan_config_with_chain(Some(chain.into()))?.map(|c| c.key);

        verify.verification_provider()?.preflight_check(verify).await?;
        Ok(())
    }

    /// Deploys the contract
    #[allow(clippy::too_many_arguments)]
    async fn deploy<M: Middleware + 'static>(
        &self,
        contract: &ContractInfo,
        abi: JsonAbi,
        bin: BytecodeObject,
        args: Vec<DynSolValue>,
        provider: M,
        chain: u64,
        zk_data: Option<(&DualCompiledContract, Vec<Vec<u8>>)>,
        signer: Option<WalletSigner>,
    ) -> Result<Address> {
        let deployer_address =
            provider.default_sender().expect("no sender address set for provider");
        let bin = bin
            .into_bytes()
            .unwrap_or_else(|| panic!("no bytecode found in bin object for {}", contract.name));
        let provider = Arc::new(provider);
        let factory = ContractFactory::new(abi.clone(), bin.clone(), provider.clone());

        let is_args_empty = args.is_empty();
        let deployer = if let Some((contract, factory_deps)) = &zk_data {
            factory.deploy_tokens_zk(args.clone(), contract).context("failed to deploy contract")
                .map(|deployer| deployer.set_zk_factory_deps(factory_deps.clone()))
        } else {
            factory.deploy_tokens(args.clone()).context("failed to deploy contract")
        }.map_err(|e| {
            if is_args_empty {
                e.wrap_err("no arguments provided for contract constructor; consider --constructor-args or --constructor-args-path")
            } else {
                e
            }
        })?;

        let is_legacy = self.tx.legacy || Chain::from(chain).is_legacy();

        let mut deployer = if is_legacy { deployer.legacy() } else { deployer };

        // set tx value if specified
        if let Some(value) = self.tx.value {
            deployer.tx.set_value(value.to_ethers());
        }

        match zk_data {
            None => provider.fill_transaction(&mut deployer.tx, None).await?,
            Some((contract, factory_deps)) => {
                let chain_id = provider.get_chainid().await?.as_u64();
                deployer.tx.set_chain_id(chain_id);

                let gas_price = provider.get_gas_price().await?;
                deployer.tx.set_gas_price(gas_price);

                deployer.tx.set_from(deployer_address);

                let nonce = provider.get_transaction_count(deployer_address, None).await?;
                deployer.tx.set_nonce(nonce);

                let constructor_args = match abi.constructor() {
                    None => Default::default(),
                    Some(constructor) => constructor.abi_encode_input(&args).unwrap_or_default(),
                };
                let data = foundry_zksync_core::encode_create_params(
                    &forge::revm::primitives::CreateScheme::Create,
                    contract.zk_bytecode_hash,
                    constructor_args,
                );
                let data = Bytes::from(data);
                deployer.tx.set_data(data.to_ethers());

                deployer
                    .tx
                    .set_to(NameOrAddress::from(foundry_zksync_core::CONTRACT_DEPLOYER_ADDRESS));

                let estimated_gas = foundry_zksync_core::estimate_gas(
                    &deployer.tx,
                    factory_deps.clone(),
                    &provider,
                )
                .await?;
                deployer.tx.set_gas(estimated_gas.limit.to_ethers());
                deployer.tx.set_gas_price(estimated_gas.price.to_ethers());
            }
        }

        // the max
        let mut priority_fee = self.tx.priority_gas_price;

        // set gas price if specified
        if let Some(gas_price) = self.tx.gas_price {
            deployer.tx.set_gas_price(gas_price.to_ethers());
        } else if !is_legacy {
            // estimate EIP1559 fees
            let (max_fee, max_priority_fee) = estimate_eip1559_fees(&provider, Some(chain))
                .await
                .wrap_err("Failed to estimate EIP1559 fees. This chain might not support EIP1559, try adding --legacy to your command.")?;
            deployer.tx.set_gas_price(max_fee);
            if priority_fee.is_none() {
                priority_fee = Some(max_priority_fee.to_alloy());
            }
        }

        // set gas limit if specified
        if let Some(gas_limit) = self.tx.gas_limit {
            deployer.tx.set_gas(gas_limit.to_ethers());
        }

        // set nonce if specified
        if let Some(nonce) = self.tx.nonce {
            deployer.tx.set_nonce(nonce.to_ethers());
        }

        // set priority fee if specified
        if let Some(priority_fee) = priority_fee {
            if is_legacy {
                eyre::bail!("there is no priority fee for legacy txs");
            }
            deployer.tx = match deployer.tx {
                TypedTransaction::Eip1559(eip1559_tx_request) => TypedTransaction::Eip1559(
                    eip1559_tx_request.max_priority_fee_per_gas(priority_fee.to_ethers()),
                ),
                _ => deployer.tx,
            };
        }

        // Before we actually deploy the contract we try check if the verify settings are valid
        let mut constructor_args = None;
        if self.verify {
            if !args.is_empty() {
                let encoded_args = abi
                    .constructor()
                    .ok_or_else(|| eyre::eyre!("could not find constructor"))?
                    .abi_encode_input(&args)?;
                constructor_args = Some(hex::encode(encoded_args));
            }

            self.verify_preflight_check(contract, constructor_args.clone(), chain).await?;
        }

        // Deploy the actual contract
        let (deployed_contract, receipt) = deployer.send_with_receipt(signer).await?;

        let address = deployed_contract;
        if self.json {
            let output = json!({
                "deployer": deployer_address.to_alloy().to_string(),
                "deployedTo": address.to_string(),
                "transactionHash": receipt.transaction_hash
            });
            println!("{output}");
        } else {
            println!("Deployer: {}", deployer_address.to_alloy());
            println!("Deployed to: {address}");
            println!("Transaction hash: {:?}", receipt.transaction_hash);
        };

        if !self.verify {
            return Ok(address);
        }

        println!("Starting contract verification...");

        let num_of_optimizations =
            if self.opts.compiler.optimize { self.opts.compiler.optimizer_runs } else { None };
        let verify = verify::VerifyArgs {
            address,
            contract: contract.clone(),
            compiler_version: None,
            constructor_args,
            constructor_args_path: None,
            num_of_optimizations,
            etherscan: EtherscanOpts { key: self.eth.etherscan.key(), chain: Some(chain.into()) },
            flatten: false,
            force: false,
            skip_is_verified_check: false,
            watch: true,
            retry: self.retry,
            libraries: vec![],
            root: None,
            verifier: self.verifier.clone(),
            via_ir: self.opts.via_ir,
            evm_version: self.opts.compiler.evm_version,
            show_standard_json_input: self.show_standard_json_input,
        };
        println!("Waiting for {} to detect contract deployment...", verify.verifier.verifier);
        verify.run().await.map(|_| address)
    }

    /// Parses the given constructor arguments into a vector of `DynSolValue`s, by matching them
    /// against the constructor's input params.
    ///
    /// Returns a list of parsed values that match the constructor's input params.
    fn parse_constructor_args(
        &self,
        constructor: &Constructor,
        constructor_args: &[String],
    ) -> Result<Vec<DynSolValue>> {
        let mut params = Vec::with_capacity(constructor.inputs.len());
        for (input, arg) in constructor.inputs.iter().zip(constructor_args) {
            // resolve the input type directly
            let ty = input
                .resolve()
                .wrap_err_with(|| format!("Could not resolve constructor arg: input={input}"))?;
            params.push((ty, arg));
        }
        let params = params.iter().map(|(ty, arg)| (ty, arg.as_str()));
        parse_tokens(params)
    }
}

/// `ContractFactory` is a [`DeploymentTxFactory`] object with an
/// [`Arc`] middleware. This type alias exists to preserve backwards
/// compatibility with less-abstract Contracts.
///
/// For full usage docs, see [`DeploymentTxFactory`].
pub type ContractFactory<M> = DeploymentTxFactory<Arc<M>, M>;

/// Helper which manages the deployment transaction of a smart contract. It
/// wraps a deployment transaction, and retrieves the contract address output
/// by it.
///
/// Currently, we recommend using the [`ContractDeployer`] type alias.
#[derive(Debug)]
#[must_use = "ContractDeploymentTx does nothing unless you `send` it"]
pub struct ContractDeploymentTx<B, M, C> {
    /// the actual deployer, exposed for overriding the defaults
    pub deployer: Deployer<B, M>,
    /// marker for the `Contract` type to create afterwards
    ///
    /// this type will be used to construct it via `From::from(Contract)`
    _contract: PhantomData<C>,
}

impl<B, M, C> Clone for ContractDeploymentTx<B, M, C>
where
    B: Clone,
{
    fn clone(&self) -> Self {
        ContractDeploymentTx { deployer: self.deployer.clone(), _contract: self._contract }
    }
}

impl<B, M, C> From<Deployer<B, M>> for ContractDeploymentTx<B, M, C> {
    fn from(deployer: Deployer<B, M>) -> Self {
        Self { deployer, _contract: PhantomData }
    }
}

/// Helper which manages the deployment transaction of a smart contract
#[derive(Debug)]
#[must_use = "Deployer does nothing unless you `send` it"]
pub struct Deployer<B, M> {
    /// The deployer's transaction, exposed for overriding the defaults
    pub tx: TypedTransaction,
    abi: JsonAbi,
    client: B,
    confs: usize,
    block: BlockNumber,
    zk_factory_deps: Option<Vec<Vec<u8>>>,
    _m: PhantomData<M>,
}

impl<B, M> Clone for Deployer<B, M>
where
    B: Clone,
{
    fn clone(&self) -> Self {
        Deployer {
            tx: self.tx.clone(),
            abi: self.abi.clone(),
            client: self.client.clone(),
            confs: self.confs,
            block: self.block,
            zk_factory_deps: self.zk_factory_deps.clone(),
            _m: PhantomData,
        }
    }
}

impl<B, M> Deployer<B, M>
where
    B: Borrow<M> + Clone,
    M: Middleware,
{
    pub fn set_zk_factory_deps(mut self, deps: Vec<Vec<u8>>) -> Self {
        self.zk_factory_deps = Some(deps);
        self
    }

    /// Uses a Legacy transaction instead of an EIP-1559 one to do the deployment
    pub fn legacy(mut self) -> Self {
        self.tx = match self.tx {
            TypedTransaction::Eip1559(inner) => {
                let tx: TransactionRequest = inner.into();
                TypedTransaction::Legacy(tx)
            }
            other => other,
        };
        self
    }

    /// Broadcasts the contract deployment transaction and after waiting for it to
    /// be sufficiently confirmed (default: 1), it returns a tuple with
    /// the [`Contract`](crate::Contract) struct at the deployed contract's address
    /// and the corresponding [`TransactionReceipt`].
    pub async fn send_with_receipt(
        self,
        signer: Option<WalletSigner>,
    ) -> Result<(Address, TransactionReceipt), ContractError<M>> {
        let pending_tx = match self.zk_factory_deps {
            None => self
                .client
                .borrow()
                .send_transaction(self.tx, Some(self.block.into()))
                .await
                .map_err(ContractError::from_middleware_error)?,
            Some(factory_deps) => {
                let tx = foundry_zksync_core::new_eip712_transaction(
                    self.tx,
                    factory_deps,
                    self.client.borrow().provider(),
                    signer.expect("No signer was found"),
                )
                .await
                .map_err(|_| ContractError::DecodingError(ethers_core::abi::Error::InvalidData))?;

                self.client
                    .borrow()
                    .send_raw_transaction(tx.to_ethers())
                    .await
                    .map_err(ContractError::from_middleware_error)?
            }
        };

        // TODO: Should this be calculated "optimistically" by address/nonce?
        let receipt = pending_tx
            .confirmations(self.confs)
            .await
            .ok()
            .flatten()
            .ok_or(ContractError::ContractNotDeployed)?;
        let address = receipt.contract_address.ok_or(ContractError::ContractNotDeployed)?;

        Ok((address.to_alloy(), receipt))
    }
}

/// To deploy a contract to the Ethereum network, a `ContractFactory` can be
/// created which manages the Contract bytecode and Application Binary Interface
/// (ABI), usually generated from the Solidity compiler.
///
/// Once the factory's deployment transaction is mined with sufficient confirmations,
/// the [`Contract`](crate::Contract) object is returned.
///
/// # Example
///
/// ```
/// # async fn foo() -> Result<(), Box<dyn std::error::Error>> {
/// use alloy_primitives::Bytes;
/// use ethers_contract::ContractFactory;
/// use ethers_providers::{Provider, Http};
///
/// // get the contract ABI and bytecode
/// let abi = Default::default();
/// let bytecode = Bytes::from_static(b"...");
///
/// // connect to the network
/// let client = Provider::<Http>::try_from("http://localhost:8545").unwrap();
/// let client = std::sync::Arc::new(client);
///
/// // create a factory which will be used to deploy instances of the contract
/// let factory = ContractFactory::new(abi, bytecode, client);
///
/// // The deployer created by the `deploy` call exposes a builder which gets consumed
/// // by the async `send` call
/// let contract = factory
///     .deploy("initial value".to_string())?
///     .confirmations(0usize)
///     .send()
///     .await?;
/// println!("{}", contract.address());
/// # Ok(())
/// # }
#[derive(Debug)]
pub struct DeploymentTxFactory<B, M> {
    client: B,
    abi: JsonAbi,
    bytecode: Bytes,
    _m: PhantomData<M>,
}

impl<B, M> Clone for DeploymentTxFactory<B, M>
where
    B: Clone,
{
    fn clone(&self) -> Self {
        DeploymentTxFactory {
            client: self.client.clone(),
            abi: self.abi.clone(),
            bytecode: self.bytecode.clone(),
            _m: PhantomData,
        }
    }
}

impl<B, M> DeploymentTxFactory<B, M>
where
    B: Borrow<M> + Clone,
    M: Middleware,
{
    /// Creates a factory for deployment of the Contract with bytecode, and the
    /// constructor defined in the abi. The client will be used to send any deployment
    /// transaction.
    pub fn new(abi: JsonAbi, bytecode: Bytes, client: B) -> Self {
        Self { client, abi, bytecode, _m: PhantomData }
    }

    /// Create a deployment tx using the provided tokens as constructor
    /// arguments
    pub fn deploy_tokens(self, params: Vec<DynSolValue>) -> Result<Deployer<B, M>, ContractError<M>>
    where
        B: Clone,
    {
        // Encode the constructor args & concatenate with the bytecode if necessary
        let data: Bytes = match (self.abi.constructor(), params.is_empty()) {
            (None, false) => return Err(ContractError::ConstructorError),
            (None, true) => self.bytecode.clone(),
            (Some(constructor), _) => {
                let input: Bytes = constructor
                    .abi_encode_input(&params)
                    .map_err(|f| {
                        ContractError::DetokenizationError(InvalidOutputType(f.to_string()))
                    })?
                    .into();
                // Concatenate the bytecode and abi-encoded constructor call.
                self.bytecode.iter().copied().chain(input).collect()
            }
        };

        // create the tx object. Since we're deploying a contract, `to` is `None`
        // We default to EIP1559 transactions, but the sender can convert it back
        // to a legacy one.
        let tx = Eip1559TransactionRequest {
            to: None,
            data: Some(data.to_ethers()),
            ..Default::default()
        };

        let tx = tx.into();

        Ok(Deployer {
            client: self.client.clone(),
            abi: self.abi,
            tx,
            confs: 1,
            block: BlockNumber::Latest,
            zk_factory_deps: None,
            _m: PhantomData,
        })
    }

    /// Create a deployment tx using the provided tokens as constructor
    /// arguments for zk networks
    pub fn deploy_tokens_zk(
        self,
        params: Vec<DynSolValue>,
        contract: &DualCompiledContract,
    ) -> Result<Deployer<B, M>, ContractError<M>>
    where
        B: Clone,
    {
        if self.abi.constructor().is_none() && !params.is_empty() {
            return Err(ContractError::ConstructorError)
        }

        // Encode the constructor args & concatenate with the bytecode if necessary
        let constructor_args = match self.abi.constructor() {
            None => Default::default(),
            Some(constructor) => constructor.abi_encode_input(&params).unwrap_or_default(),
        };
        let data: Bytes = foundry_zksync_core::encode_create_params(
            &forge::revm::primitives::CreateScheme::Create,
            contract.zk_bytecode_hash,
            constructor_args,
        )
        .into();

        let tx = Eip1559TransactionRequest {
            to: Some(NameOrAddress::from(foundry_zksync_core::CONTRACT_DEPLOYER_ADDRESS)),
            data: Some(data.to_ethers()),
            ..Default::default()
        };

        Ok(Deployer {
            client: self.client.clone(),
            abi: self.abi,
            tx: tx.into(),
            confs: 1,
            block: BlockNumber::Latest,
            zk_factory_deps: Some(vec![contract.zk_deployed_bytecode.clone()]),
            _m: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_parse_create() {
        let args: CreateArgs = CreateArgs::parse_from([
            "foundry-cli",
            "src/Domains.sol:Domains",
            "--verify",
            "--retries",
            "10",
            "--delay",
            "30",
        ]);
        assert_eq!(args.retry.retries, 10);
        assert_eq!(args.retry.delay, 30);
    }
    #[test]
    fn can_parse_chain_id() {
        let args: CreateArgs = CreateArgs::parse_from([
            "foundry-cli",
            "src/Domains.sol:Domains",
            "--verify",
            "--retries",
            "10",
            "--delay",
            "30",
            "--chain-id",
            "9999",
        ]);
        assert_eq!(args.chain_id(), Some(9999));
    }

    #[test]
    fn test_parse_constructor_args() {
        let args: CreateArgs = CreateArgs::parse_from([
            "foundry-cli",
            "src/Domains.sol:Domains",
            "--constructor-args",
            "Hello",
        ]);
        let constructor: Constructor = serde_json::from_str(r#"{"type":"constructor","inputs":[{"name":"_name","type":"string","internalType":"string"}],"stateMutability":"nonpayable"}"#).unwrap();
        let params = args.parse_constructor_args(&constructor, &args.constructor_args).unwrap();
        assert_eq!(params, vec![DynSolValue::String("Hello".to_string())]);
    }

    #[test]
    fn test_parse_tuple_constructor_args() {
        let args: CreateArgs = CreateArgs::parse_from([
            "foundry-cli",
            "src/Domains.sol:Domains",
            "--constructor-args",
            "[(1,2), (2,3), (3,4)]",
        ]);
        let constructor: Constructor = serde_json::from_str(r#"{"type":"constructor","inputs":[{"name":"_points","type":"tuple[]","internalType":"struct Point[]","components":[{"name":"x","type":"uint256","internalType":"uint256"},{"name":"y","type":"uint256","internalType":"uint256"}]}],"stateMutability":"nonpayable"}"#).unwrap();
        let _params = args.parse_constructor_args(&constructor, &args.constructor_args).unwrap();
    }
}
