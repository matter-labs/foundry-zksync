//! Forge test runner for multiple contracts.

use crate::{
    link::{LinkOutput, Linker},
    result::SuiteResult,
    ContractRunner, TestFilter, TestOptions,
};
use alloy_json_abi::{Function, JsonAbi};
use alloy_primitives::{Address, Bytes, U256};
use eyre::Result;
use foundry_common::{get_contract_name, ContractsByArtifact, TestFunctionExt};
use foundry_compilers::{
    contracts::ArtifactContracts,
    zksync::compile::output::ProjectCompileOutput as ZkProjectCompileOutput, Artifact, ArtifactId,
    ArtifactOutput, ConfigurableArtifacts, ProjectCompileOutput,
};
use foundry_evm::{
    backend::Backend,
    decode::RevertDecoder,
    executors::{Executor, ExecutorBuilder},
    fork::CreateFork,
    inspectors::CheatsConfig,
    opts::EvmOpts,
    revm,
};
use rayon::prelude::*;
use revm::primitives::SpecId;

use std::{
    collections::BTreeMap,
    fmt::Debug,
    path::Path,
    sync::{mpsc, Arc},
};

pub type DeployableContracts = BTreeMap<ArtifactId, (JsonAbi, Bytes, Vec<Bytes>)>;

/// A multi contract runner receives a set of contracts deployed in an EVM instance and proceeds
/// to run all test functions in these contracts.
pub struct MultiContractRunner {
    /// Mapping of contract name to JsonAbi, creation bytecode and library bytecode which
    /// needs to be deployed & linked against
    pub contracts: DeployableContracts,
    /// Compiled contracts by name that have an JsonAbi and runtime bytecode
    pub known_contracts: ContractsByArtifact,
    /// The EVM instance used in the test runner
    pub evm_opts: EvmOpts,
    /// The configured evm
    pub env: revm::primitives::Env,
    /// The EVM spec
    pub evm_spec: SpecId,
    /// Revert decoder. Contains all known errors and their selectors.
    pub revert_decoder: RevertDecoder,
    /// The address which will be used as the `from` field in all EVM calls
    pub sender: Option<Address>,
    /// A map of contract names to absolute source file paths
    pub source_paths: BTreeMap<String, String>,
    /// The fork to use at launch
    pub fork: Option<CreateFork>,
    /// Additional cheatcode inspector related settings derived from the `Config`
    pub cheats_config: Arc<CheatsConfig>,
    /// Whether to collect coverage info
    pub coverage: bool,
    /// Whether to collect debug info
    pub debug: bool,
    /// Settings related to fuzz and/or invariant tests
    pub test_options: TestOptions,
    /// Whether to enable call isolation
    pub isolation: bool,
    pub use_zk: bool,
}

impl MultiContractRunner {
    /// Returns the number of matching tests
    pub fn matching_test_function_count(&self, filter: &dyn TestFilter) -> usize {
        self.matching_test_functions(filter).count()
    }

    /// Returns all test functions matching the filter
    pub fn matching_test_functions<'a>(
        &'a self,
        filter: &'a dyn TestFilter,
    ) -> impl Iterator<Item = &Function> {
        self.contracts
            .iter()
            .filter(|(id, _)| filter.matches_path(&id.source) && filter.matches_contract(&id.name))
            .flat_map(|(_, (abi, _, _))| {
                abi.functions().filter(|func| filter.matches_test(&func.signature()))
            })
    }

    /// Get an iterator over all test contract functions that matches the filter path and contract
    /// name
    fn filtered_tests<'a>(&'a self, filter: &'a dyn TestFilter) -> impl Iterator<Item = &Function> {
        self.contracts
            .iter()
            .filter(|(id, _)| filter.matches_path(&id.source) && filter.matches_contract(&id.name))
            .flat_map(|(_, (abi, _, _))| abi.functions())
    }

    /// Get all test names matching the filter
    pub fn get_tests(&self, filter: &dyn TestFilter) -> Vec<String> {
        self.filtered_tests(filter)
            .map(|func| func.name.clone())
            .filter(|name| name.is_test())
            .collect()
    }

    /// Returns all matching tests grouped by contract grouped by file (file -> (contract -> tests))
    pub fn list(&self, filter: &dyn TestFilter) -> BTreeMap<String, BTreeMap<String, Vec<String>>> {
        self.contracts
            .iter()
            .filter(|(id, _)| filter.matches_path(&id.source) && filter.matches_contract(&id.name))
            .filter(|(_, (abi, _, _))| abi.functions().any(|func| filter.matches_test(&func.name)))
            .map(|(id, (abi, _, _))| {
                let source = id.source.as_path().display().to_string();
                let name = id.name.clone();
                let tests = abi
                    .functions()
                    .filter(|func| func.name.is_test())
                    .filter(|func| filter.matches_test(&func.signature()))
                    .map(|func| func.name.clone())
                    .collect::<Vec<_>>();

                (source, name, tests)
            })
            .fold(BTreeMap::new(), |mut acc, (source, name, tests)| {
                acc.entry(source).or_default().insert(name, tests);
                acc
            })
    }

    /// Executes _all_ tests that match the given `filter`.
    ///
    /// The same as [`test`](Self::test), but returns the results instead of streaming them.
    ///
    /// Note that this method returns only when all tests have been executed.
    pub async fn test_collect(
        &mut self,
        filter: &dyn TestFilter,
        test_options: TestOptions,
    ) -> BTreeMap<String, SuiteResult> {
        self.test_iter(filter, test_options).await.collect()
    }

    /// Executes _all_ tests that match the given `filter`.
    ///
    /// The same as [`test`](Self::test), but returns the results instead of streaming them.
    ///
    /// Note that this method returns only when all tests have been executed.
    pub async fn test_iter(
        &mut self,
        filter: &dyn TestFilter,
        test_options: TestOptions,
    ) -> impl Iterator<Item = (String, SuiteResult)> {
        let (tx, rx) = mpsc::channel();
        self.test(filter, tx, test_options).await;
        rx.into_iter()
    }

    /// Executes _all_ tests that match the given `filter`.
    ///
    /// This will create the runtime based on the configured `evm` ops and create the `Backend`
    /// before executing all contracts and their tests in _parallel_.
    ///
    /// Each Executor gets its own instance of the `Backend`.
    pub async fn test(
        &mut self,
        filter: &dyn TestFilter,
        stream_result: mpsc::Sender<(String, SuiteResult)>,
        test_options: TestOptions,
    ) {
        trace!("running all tests");

        // the db backend that serves all the data, each contract gets its own instance
        let mut db = Backend::spawn(self.fork.take()).await;
        db.is_zk = self.cheats_config.use_zk;

        self.contracts
            .par_iter()
            .filter(|(id, _)| filter.matches_path(&id.source) && filter.matches_contract(&id.name))
            .filter(|(_, (abi, _, _))| abi.functions().any(|func| filter.matches_test(&func.name)))
            .for_each_with(stream_result, |stream_result, (id, (abi, deploy_code, libs))| {
                let executor = ExecutorBuilder::new()
                    .inspectors(|stack| {
                        stack
                            .cheatcodes(self.cheats_config.clone())
                            .trace(self.evm_opts.verbosity >= 3 || self.debug)
                            .debug(self.debug)
                            .coverage(self.coverage)
                            .enable_isolation(self.isolation)
                    })
                    .spec(self.evm_spec)
                    .gas_limit(self.evm_opts.gas_limit())
                    .use_zk_vm(self.use_zk)
                    .build(self.env.clone(), db.clone());
                let identifier = id.identifier();
                trace!(contract=%identifier, "start executing all tests in contract");

                let result = self.run_tests(
                    &identifier,
                    abi,
                    executor,
                    deploy_code.clone(),
                    libs,
                    filter,
                    test_options.clone(),
                );
                trace!(contract=?identifier, "executed all tests in contract");

                let _ = stream_result.send((identifier, result));
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn run_tests(
        &self,
        name: &str,
        contract: &JsonAbi,
        executor: Executor,
        deploy_code: Bytes,
        libs: &[Bytes],
        filter: &dyn TestFilter,
        test_options: TestOptions,
    ) -> SuiteResult {
        let span = info_span!("run_tests");
        if !span.is_disabled() {
            if enabled!(tracing::Level::TRACE) {
                span.record("contract", name);
            } else {
                span.record("contract", get_contract_name(name));
            }
        }
        let _guard = span.enter();

        let runner = ContractRunner::new(
            name,
            executor,
            contract,
            deploy_code,
            self.evm_opts.initial_balance,
            self.sender,
            &self.revert_decoder,
            libs,
            self.debug,
        );
        runner.run_tests(filter, test_options, Some(&self.known_contracts))
    }
}

/// Builder used for instantiating the multi-contract runner
#[derive(Clone, Debug, Default)]
#[must_use = "builders do nothing unless you call `build` on them"]
pub struct MultiContractRunnerBuilder {
    /// The address which will be used to deploy the initial contracts and send all
    /// transactions
    pub sender: Option<Address>,
    /// The initial balance for each one of the deployed smart contracts
    pub initial_balance: U256,
    /// The EVM spec to use
    pub evm_spec: Option<SpecId>,
    /// The fork to use at launch
    pub fork: Option<CreateFork>,
    /// Additional cheatcode inspector related settings derived from the `Config`
    pub cheats_config: Option<CheatsConfig>,
    /// Whether or not to collect coverage info
    pub coverage: bool,
    /// Whether or not to collect debug info
    pub debug: bool,
    /// Whether to enable call isolation
    pub isolation: bool,
    /// Settings related to fuzz and/or invariant tests
    pub test_options: Option<TestOptions>,
}

#[derive(Debug)]
pub struct ProjectCompileDualOutput<A: ArtifactOutput = ConfigurableArtifacts> {
    pub zk_output: Option<ProjectCompileOutput<A>>,
    pub solc_output: Option<ProjectCompileOutput<A>>,
}

impl<A: ArtifactOutput> ProjectCompileDualOutput<A> {
    pub fn only_zk(zk_output: ProjectCompileOutput<A>) -> Self {
        ProjectCompileDualOutput { zk_output: Some(zk_output), solc_output: None }
    }
    pub fn only_evm(solc_output: ProjectCompileOutput<A>) -> Self {
        ProjectCompileDualOutput { zk_output: None, solc_output: Some(solc_output) }
    }
}

impl MultiContractRunnerBuilder {
    pub fn sender(mut self, sender: Address) -> Self {
        self.sender = Some(sender);
        self
    }

    pub fn initial_balance(mut self, initial_balance: U256) -> Self {
        self.initial_balance = initial_balance;
        self
    }

    pub fn evm_spec(mut self, spec: SpecId) -> Self {
        self.evm_spec = Some(spec);
        self
    }

    pub fn with_fork(mut self, fork: Option<CreateFork>) -> Self {
        self.fork = fork;
        self
    }

    pub fn with_cheats_config(mut self, cheats_config: CheatsConfig) -> Self {
        self.cheats_config = Some(cheats_config);
        self
    }

    pub fn with_test_options(mut self, test_options: TestOptions) -> Self {
        self.test_options = Some(test_options);
        self
    }

    pub fn set_coverage(mut self, enable: bool) -> Self {
        self.coverage = enable;
        self
    }

    pub fn set_debug(mut self, enable: bool) -> Self {
        self.debug = enable;
        self
    }

    pub fn enable_isolation(mut self, enable: bool) -> Self {
        self.isolation = enable;
        self
    }

    /// Given an EVM, proceeds to return a runner which is able to execute all tests
    /// against that evm
    pub fn build(
        self,
        root: &Path,
        output: ProjectCompileOutput,
        zk_output: Option<ZkProjectCompileOutput>,
        env: revm::primitives::Env,
        evm_opts: EvmOpts,
    ) -> Result<MultiContractRunner> {
        let use_zk = zk_output.is_some();
        let mut known_contracts = ContractsByArtifact::default();

        // This is just the contracts compiled, but we need to merge this with the read cached
        // artifacts.
        let contracts = output
            .with_stripped_file_prefixes(root)
            .into_artifacts()
            .map(|(i, c)| (i, c.into_contract_bytecode()))
            .collect::<ArtifactContracts>();

        let source_paths = contracts
            .iter()
            .map(|(i, _)| (i.identifier(), root.join(&i.source).to_string_lossy().into()))
            .collect::<BTreeMap<String, String>>();

        let linker = Linker::new(root, contracts);

        // Create a mapping of name => (abi, deployment code, Vec<library deployment code>)
        let mut deployable_contracts = DeployableContracts::default();

        for (id, contract) in &linker.contracts.0 {
            let Some(abi) = contract.abi.as_ref() else {
                continue;
            };

            let LinkOutput { libs_to_deploy, libraries } =
                linker.link_with_nonce_or_address(Default::default(), evm_opts.sender, 1, id)?;

            let linked_contract = linker.link(id, &libraries)?;

            // get bytes if deployable, else add to known contracts and continue.
            // interfaces and abstract contracts should be known to enable fuzzing of their ABI
            // but they should not be deployable and their source code should be skipped by the
            // debugger and linker.
            let Some(bytecode) = linked_contract
                .get_bytecode_bytes()
                .map(|b| b.into_owned())
                .filter(|b| !b.is_empty())
            else {
                known_contracts.insert(id.clone(), (abi.clone(), vec![]));
                continue;
            };

            // if it's a test, add it to deployable contracts
            if abi.constructor.as_ref().map(|c| c.inputs.is_empty()).unwrap_or(true) &&
                abi.functions().any(|func| func.name.is_test() || func.name.is_invariant_test())
            {
                deployable_contracts.insert(id.clone(), (abi.clone(), bytecode, libs_to_deploy));
            }

            if !use_zk {
                if let Some(bytes) = linked_contract.get_deployed_bytecode_bytes() {
                    known_contracts.insert(id.clone(), (abi.clone(), bytes.to_vec()));
                }
            }
        }
        if let Some(zk_output) = zk_output {
            let zk_contracts = zk_output.with_stripped_file_prefixes(root).into_artifacts();
            for (id, contract) in zk_contracts {
                if let Some(metadata) = contract.metadata {
                    if let Some(solc_metadata_value) =
                        metadata.get("solc_metadata").and_then(serde_json::Value::as_str)
                    {
                        if let Ok(solc_metadata_json) =
                            serde_json::from_str::<serde_json::Value>(solc_metadata_value)
                        {
                            let abi_json = &solc_metadata_json["output"]["abi"];
                            let abi_string = abi_json.to_string();
                            let abi: JsonAbi = JsonAbi::from_json_str(&abi_string)?;
                            let bytecode = contract
                                .bytecode
                                .as_ref()
                                .and_then(|b| b.object.as_bytes())
                                .map_or_else(Vec::new, |b| b.to_vec());
                            known_contracts.insert(id, (abi.clone(), bytecode));
                        }
                    }
                }
            }
        }

        let revert_decoder =
            RevertDecoder::new().with_abis(known_contracts.values().map(|(abi, _)| abi));
        Ok(MultiContractRunner {
            contracts: deployable_contracts,
            known_contracts,
            evm_opts,
            env,
            evm_spec: self.evm_spec.unwrap_or(SpecId::MERGE),
            sender: self.sender,
            revert_decoder,
            source_paths,
            fork: self.fork,
            cheats_config: self.cheats_config.unwrap_or_default().into(),
            coverage: self.coverage,
            debug: self.debug,
            test_options: self.test_options.unwrap_or_default(),
            isolation: self.isolation,
            use_zk,
        })
    }
}
