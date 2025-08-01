//! The Forge test runner.

use crate::{
    MultiContractRunner, TestFilter,
    fuzz::{BaseCounterExample, invariant::BasicTxDetails},
    multi_runner::{TestContract, TestRunnerConfig, is_matching_test},
    progress::{TestsProgress, start_fuzz_progress},
    result::{SuiteResult, TestResult, TestSetup},
};
use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
use alloy_json_abi::Function;
use alloy_primitives::{Address, Bytes, U256, address, map::HashMap};
use eyre::Result;
use foundry_common::{TestFunctionExt, TestFunctionKind, contracts::ContractsByAddress};
use foundry_compilers::utils::canonicalized;
use foundry_config::{Config, InvariantConfig};
use foundry_evm::{
    constants::CALLER,
    decode::RevertDecoder,
    executors::{
        CallResult, EvmError, Executor, ITest, RawCallResult,
        fuzz::FuzzedExecutor,
        invariant::{
            InvariantExecutor, InvariantFuzzError, check_sequence, replay_error, replay_run,
        },
        strategy::DeployLibKind,
    },
    fuzz::{
        CounterExample, FuzzFixtures, fixture_name,
        invariant::{CallDetails, InvariantContract},
    },
    traces::{TraceKind, TraceMode, load_contracts},
};
use itertools::Itertools;
use proptest::test_runner::{
    FailurePersistence, FileFailurePersistence, RngAlgorithm, TestError, TestRng, TestRunner,
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    cmp::min,
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
use tracing::Span;

/// When running tests, we deploy all external libraries present in the project. To avoid additional
/// libraries affecting nonces of senders used in tests, we are using separate address to
/// predeploy libraries.
///
/// `address(uint160(uint256(keccak256("foundry library deployer"))))`
pub const LIBRARY_DEPLOYER: Address = address!("0x1F95D37F27EA0dEA9C252FC09D5A6eaA97647353");

/// A type that executes all tests of a contract
pub struct ContractRunner<'a> {
    /// The name of the contract.
    name: &'a str,
    /// The data of the contract.
    contract: &'a TestContract,
    /// The EVM executor.
    executor: Executor,
    /// Overall test run progress.
    progress: Option<&'a TestsProgress>,
    /// The handle to the tokio runtime.
    tokio_handle: &'a tokio::runtime::Handle,
    /// The span of the contract.
    span: tracing::Span,
    /// The contract-level configuration.
    tcfg: Cow<'a, TestRunnerConfig>,
    /// The parent runner.
    mcr: &'a MultiContractRunner,
}

impl<'a> std::ops::Deref for ContractRunner<'a> {
    type Target = Cow<'a, TestRunnerConfig>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.tcfg
    }
}

impl<'a> ContractRunner<'a> {
    pub fn new(
        name: &'a str,
        contract: &'a TestContract,
        executor: Executor,
        progress: Option<&'a TestsProgress>,
        tokio_handle: &'a tokio::runtime::Handle,
        span: Span,
        mcr: &'a MultiContractRunner,
    ) -> Self {
        Self {
            name,
            contract,
            executor,
            progress,
            tokio_handle,
            span,
            tcfg: Cow::Borrowed(&mcr.tcfg),
            mcr,
        }
    }

    /// Deploys the test contract inside the runner from the sending account, and optionally runs
    /// the `setUp` function on the test contract.
    pub fn setup(&mut self, call_setup: bool) -> TestSetup {
        self._setup(call_setup).unwrap_or_else(|err| {
            if err.to_string().contains("skipped") {
                TestSetup::skipped(err.to_string())
            } else {
                TestSetup::failed(err.to_string())
            }
        })
    }

    fn _setup(&mut self, call_setup: bool) -> Result<TestSetup> {
        trace!(call_setup, "setting up");

        self.apply_contract_inline_config()?;

        // We max out their balance so that they can deploy and make calls.
        self.executor.set_balance(self.sender, U256::MAX)?;
        self.executor.set_balance(CALLER, U256::MAX)?;

        // We set the nonce of the deployer accounts to 1 to get the same addresses as DappTools.
        self.executor.set_nonce(self.sender, 1)?;

        // Deploy libraries.
        self.executor.set_balance(LIBRARY_DEPLOYER, U256::MAX)?;

        let mut result = TestSetup::default();
        for code in &self.mcr.libs_to_deploy {
            let deploy_result = self.executor.deploy_library(
                LIBRARY_DEPLOYER,
                DeployLibKind::Create(code.clone()),
                U256::ZERO,
                Some(&self.mcr.revert_decoder),
            );

            let deployments = match deploy_result {
                Err(err) => vec![Err(err)],
                Ok(deployments) => deployments.into_iter().map(Ok).collect(),
            };

            for deploy_result in
                deployments.into_iter().map(|result| result.map(|deployment| deployment.result))
            {
                // Record deployed library address.
                if let Ok(deployed) = &deploy_result {
                    result.deployed_libs.push(deployed.address);
                }

                let (raw, reason) = RawCallResult::from_evm_result(deploy_result.map(Into::into))?;
                result.extend(raw, TraceKind::Deployment);
                if reason.is_some() {
                    result.reason = reason;
                    return Ok(result);
                }
            }
        }

        let address = self.sender.create(self.executor.get_nonce(self.sender)?);
        result.address = address;
        // NOTE(zk): the test contract is set here instead of where upstream does it as
        // the test contract address needs to be retrieved in order to skip
        // zkEVM mode for the creation of the test address (and for calls to it later).
        self.executor.backend_mut().set_test_contract(address);

        // Set the contracts initial balance before deployment, so it is available during
        // construction
        self.executor.set_balance(address, self.initial_balance())?;

        // Deploy the test contract
        let deploy_result = self.executor.deploy(
            self.sender,
            self.contract.bytecode.clone(),
            U256::ZERO,
            Some(&self.mcr.revert_decoder),
        );

        result.deployment_failure = deploy_result.is_err();

        if let Ok(dr) = &deploy_result {
            debug_assert_eq!(dr.address, address);
        }
        let (raw, reason) = RawCallResult::from_evm_result(deploy_result.map(Into::into))?;
        result.extend(raw, TraceKind::Deployment);
        if reason.is_some() {
            result.reason = reason;
            return Ok(result);
        }

        // Reset `self.sender`s, `CALLER`s and `LIBRARY_DEPLOYER`'s balance to the initial balance.
        self.executor.set_balance(self.sender, self.initial_balance())?;
        self.executor.set_balance(CALLER, self.initial_balance())?;
        self.executor.set_balance(LIBRARY_DEPLOYER, self.initial_balance())?;

        self.executor.deploy_create2_deployer()?;

        // Test contract has already been deployed so we can migrate the database to zkEVM storage
        // in the next runner execution. Additionally we can allow persisting the next nonce update
        // to simulate EVM behavior where only the tx that deploys the test contract increments the
        // nonce.
        if let Some(cheatcodes) = &mut self.executor.inspector.cheatcodes {
            debug!("test contract deployed");
            cheatcodes.strategy.runner.base_contract_deployed(cheatcodes.strategy.context.as_mut());
        }

        // Optionally call the `setUp` function
        if call_setup {
            trace!("calling setUp");
            let res = self.executor.setup(None, address, Some(&self.mcr.revert_decoder));
            let (raw, reason) = RawCallResult::from_evm_result(res)?;
            result.extend(raw, TraceKind::Setup);
            result.reason = reason;
        }

        result.fuzz_fixtures = self.fuzz_fixtures(address);

        Ok(result)
    }

    fn initial_balance(&self) -> U256 {
        self.evm_opts.initial_balance
    }

    /// Configures this runner with the inline configuration for the contract.
    fn apply_contract_inline_config(&mut self) -> Result<()> {
        if self.inline_config.contains_contract(self.name) {
            let new_config = Arc::new(self.inline_config(None)?);
            self.tcfg.to_mut().reconfigure_with(new_config);
            let prev_tracer = self.executor.inspector_mut().tracer.take();
            self.tcfg.configure_executor(&mut self.executor);
            // Don't set tracer here.
            self.executor.inspector_mut().tracer = prev_tracer;
        }
        Ok(())
    }

    /// Returns the configuration for a contract or function.
    fn inline_config(&self, func: Option<&Function>) -> Result<Config> {
        let function = func.map(|f| f.name.as_str()).unwrap_or("");
        let config =
            self.mcr.inline_config.merge(self.name, function, &self.config).extract::<Config>()?;
        Ok(config)
    }

    /// Collect fixtures from test contract.
    ///
    /// Fixtures can be defined:
    /// - as storage arrays in test contract, prefixed with `fixture`
    /// - as functions prefixed with `fixture` and followed by parameter name to be fuzzed
    ///
    /// Storage array fixtures:
    /// `uint256[] public fixture_amount = [1, 2, 3];`
    /// define an array of uint256 values to be used for fuzzing `amount` named parameter in scope
    /// of the current test.
    ///
    /// Function fixtures:
    /// `function fixture_owner() public returns (address[] memory){}`
    /// returns an array of addresses to be used for fuzzing `owner` named parameter in scope of the
    /// current test.
    fn fuzz_fixtures(&mut self, address: Address) -> FuzzFixtures {
        let mut fixtures = HashMap::default();
        let fixture_functions = self.contract.abi.functions().filter(|func| func.is_fixture());
        for func in fixture_functions {
            if func.inputs.is_empty() {
                // Read fixtures declared as functions.
                if let Ok(CallResult { raw: _, decoded_result }) =
                    self.executor.call(CALLER, address, func, &[], U256::ZERO, None)
                {
                    fixtures.insert(fixture_name(func.name.clone()), decoded_result);
                }
            } else {
                // For reading fixtures from storage arrays we collect values by calling the
                // function with incremented indexes until there's an error.
                let mut vals = Vec::new();
                let mut index = 0;
                loop {
                    if let Ok(CallResult { raw: _, decoded_result }) = self.executor.call(
                        CALLER,
                        address,
                        func,
                        &[DynSolValue::Uint(U256::from(index), 256)],
                        U256::ZERO,
                        None,
                    ) {
                        vals.push(decoded_result);
                    } else {
                        // No result returned for this index, we reached the end of storage
                        // array or the function is not a valid fixture.
                        break;
                    }
                    index += 1;
                }
                fixtures.insert(fixture_name(func.name.clone()), DynSolValue::Array(vals));
            };
        }
        FuzzFixtures::new(fixtures)
    }

    /// Runs all tests for a contract whose names match the provided regular expression
    pub fn run_tests(mut self, filter: &dyn TestFilter) -> SuiteResult {
        let start = Instant::now();
        let mut warnings = Vec::new();

        // Check if `setUp` function with valid signature declared.
        let setup_fns: Vec<_> =
            self.contract.abi.functions().filter(|func| func.name.is_setup()).collect();
        let call_setup = setup_fns.len() == 1 && setup_fns[0].name == "setUp";
        // There is a single miss-cased `setUp` function, so we add a warning
        for &setup_fn in &setup_fns {
            if setup_fn.name != "setUp" {
                warnings.push(format!(
                    "Found invalid setup function \"{}\" did you mean \"setUp()\"?",
                    setup_fn.signature()
                ));
            }
        }

        // There are multiple setUp function, so we return a single test result for `setUp`
        if setup_fns.len() > 1 {
            return SuiteResult::new(
                start.elapsed(),
                [("setUp()".to_string(), TestResult::fail("multiple setUp functions".to_string()))]
                    .into(),
                warnings,
            );
        }

        // Check if `afterInvariant` function with valid signature declared.
        let after_invariant_fns: Vec<_> =
            self.contract.abi.functions().filter(|func| func.name.is_after_invariant()).collect();
        if after_invariant_fns.len() > 1 {
            // Return a single test result failure if multiple functions declared.
            return SuiteResult::new(
                start.elapsed(),
                [(
                    "afterInvariant()".to_string(),
                    TestResult::fail("multiple afterInvariant functions".to_string()),
                )]
                .into(),
                warnings,
            );
        }
        let call_after_invariant = after_invariant_fns.first().is_some_and(|after_invariant_fn| {
            let match_sig = after_invariant_fn.name == "afterInvariant";
            if !match_sig {
                warnings.push(format!(
                    "Found invalid afterInvariant function \"{}\" did you mean \"afterInvariant()\"?",
                    after_invariant_fn.signature()
                ));
            }
            match_sig
        });

        // Invariant testing requires tracing to figure out what contracts were created.
        // We also want to disable `debug` for setup since we won't be using those traces.
        let has_invariants = self.contract.abi.functions().any(|func| func.is_invariant_test());

        let prev_tracer = self.executor.inspector_mut().tracer.take();
        if prev_tracer.is_some() || has_invariants {
            self.executor.set_tracing(TraceMode::Call);
        }

        let setup_time = Instant::now();
        let setup = self.setup(call_setup);
        debug!("finished setting up in {:?}", setup_time.elapsed());

        self.executor.inspector_mut().tracer = prev_tracer;

        if setup.reason.is_some() {
            // The setup failed, so we return a single test result for `setUp`
            let fail_msg = if !setup.deployment_failure {
                "setUp()".to_string()
            } else {
                "constructor()".to_string()
            };
            return SuiteResult::new(
                start.elapsed(),
                [(fail_msg, TestResult::setup_result(setup))].into(),
                warnings,
            );
        }

        // Filter out functions sequentially since it's very fast and there is no need to do it
        // in parallel.
        let find_timer = Instant::now();
        let functions = self
            .contract
            .abi
            .functions()
            .filter(|func| is_matching_test(func, filter))
            .collect::<Vec<_>>();
        debug!(
            "Found {} test functions out of {} in {:?}",
            functions.len(),
            self.contract.abi.functions().count(),
            find_timer.elapsed(),
        );

        let identified_contracts = has_invariants.then(|| {
            load_contracts(setup.traces.iter().map(|(_, t)| &t.arena), &self.mcr.known_contracts)
        });

        let test_fail_instances = functions
            .iter()
            .filter_map(|func| {
                TestFunctionKind::classify(&func.name, !func.inputs.is_empty())
                    .is_any_test_fail()
                    .then_some(func.name.clone())
            })
            .collect::<Vec<_>>();

        if !test_fail_instances.is_empty() {
            let instances = format!(
                "Found {} instances: {}",
                test_fail_instances.len(),
                test_fail_instances.join(", ")
            );
            let fail =  TestResult::fail("`testFail*` has been removed. Consider changing to test_Revert[If|When]_Condition and expecting a revert".to_string());
            return SuiteResult::new(start.elapsed(), [(instances, fail)].into(), warnings);
        }

        let test_results = functions
            .par_iter()
            .map(|&func| {
                let start = Instant::now();

                let _guard = self.tokio_handle.enter();

                let _guard;
                let current_span = tracing::Span::current();
                if current_span.is_none() || current_span.id() != self.span.id() {
                    _guard = self.span.enter();
                }

                let sig = func.signature();
                let kind = func.test_function_kind();

                let _guard = debug_span!(
                    "test",
                    %kind,
                    name = %if enabled!(tracing::Level::TRACE) { &sig } else { &func.name },
                )
                .entered();

                let mut res = FunctionRunner::new(&self, &setup).run(
                    func,
                    kind,
                    call_after_invariant,
                    identified_contracts.as_ref(),
                );
                res.duration = start.elapsed();

                (sig, res)
            })
            .collect::<BTreeMap<_, _>>();

        let duration = start.elapsed();
        SuiteResult::new(duration, test_results, warnings)
    }
}

/// Executes a single test function, returning a [`TestResult`].
struct FunctionRunner<'a> {
    /// The function-level configuration.
    tcfg: Cow<'a, TestRunnerConfig>,
    /// The EVM executor.
    executor: Cow<'a, Executor>,
    /// The parent runner.
    cr: &'a ContractRunner<'a>,
    /// The address of the test contract.
    address: Address,
    /// The test setup result.
    setup: &'a TestSetup,
    /// The test result. Returned after running the test.
    result: TestResult,
}

impl<'a> std::ops::Deref for FunctionRunner<'a> {
    type Target = Cow<'a, TestRunnerConfig>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.tcfg
    }
}

impl<'a> FunctionRunner<'a> {
    fn new(cr: &'a ContractRunner<'a>, setup: &'a TestSetup) -> Self {
        Self {
            tcfg: match &cr.tcfg {
                Cow::Borrowed(tcfg) => Cow::Borrowed(tcfg),
                Cow::Owned(tcfg) => Cow::Owned(tcfg.clone()),
            },
            executor: Cow::Borrowed(&cr.executor),
            cr,
            address: setup.address,
            setup,
            result: TestResult::new(setup),
        }
    }

    fn revert_decoder(&self) -> &'a RevertDecoder {
        &self.cr.mcr.revert_decoder
    }

    /// Configures this runner with the inline configuration for the contract.
    fn apply_function_inline_config(&mut self, func: &Function) -> Result<()> {
        if self.inline_config.contains_function(self.cr.name, &func.name) {
            let new_config = Arc::new(self.cr.inline_config(Some(func))?);
            self.tcfg.to_mut().reconfigure_with(new_config);
            self.tcfg.configure_executor(self.executor.to_mut());
        }
        Ok(())
    }

    fn run(
        mut self,
        func: &Function,
        kind: TestFunctionKind,
        call_after_invariant: bool,
        identified_contracts: Option<&ContractsByAddress>,
    ) -> TestResult {
        if let Err(e) = self.apply_function_inline_config(func) {
            self.result.single_fail(Some(e.to_string()));
            return self.result;
        }

        match kind {
            TestFunctionKind::UnitTest { .. } => self.run_unit_test(func),
            TestFunctionKind::FuzzTest { .. } => self.run_fuzz_test(func),
            TestFunctionKind::TableTest => self.run_table_test(func),
            TestFunctionKind::InvariantTest => {
                let test_bytecode = &self.cr.contract.bytecode;
                self.run_invariant_test(
                    func,
                    call_after_invariant,
                    identified_contracts.unwrap(),
                    test_bytecode,
                )
            }
            _ => unreachable!(),
        }
    }

    /// Runs a single unit test.
    ///
    /// Applies before test txes (if any), runs current test and returns the `TestResult`.
    ///
    /// Before test txes are applied in order and state modifications committed to the EVM database
    /// (therefore the unit test call will be made on modified state).
    /// State modifications of before test txes and unit test function call are discarded after
    /// test ends, similar to `eth_call`.
    fn run_unit_test(mut self, func: &Function) -> TestResult {
        // Prepare unit test execution.
        if self.prepare_test(func).is_err() {
            return self.result;
        }

        // Run current unit test.
        let (mut raw_call_result, reason) = match self.executor.call(
            self.sender,
            self.address,
            func,
            &[],
            U256::ZERO,
            Some(self.revert_decoder()),
        ) {
            Ok(res) => (res.raw, None),
            Err(EvmError::Execution(err)) => (err.raw, Some(err.reason)),
            Err(EvmError::Skip(reason)) => {
                self.result.single_skip(reason);
                return self.result;
            }
            Err(err) => {
                self.result.single_fail(Some(err.to_string()));
                return self.result;
            }
        };

        let success =
            self.executor.is_raw_call_mut_success(self.address, &mut raw_call_result, false);
        self.result.single_result(success, reason, raw_call_result);
        self.result
    }

    /// Runs a table test.
    /// The parameters dataset (table) is created from defined parameter fixtures, therefore each
    /// test table parameter should have the same number of fixtures defined.
    /// E.g. for table test
    /// - `table_test(uint256 amount, bool swap)` fixtures are defined as
    /// - `uint256[] public fixtureAmount = [2, 5]`
    /// - `bool[] public fixtureSwap = [true, false]` The `table_test` is then called with the pair
    ///   of args `(2, true)` and `(5, false)`.
    fn run_table_test(mut self, func: &Function) -> TestResult {
        // Prepare unit test execution.
        if self.prepare_test(func).is_err() {
            return self.result;
        }

        // Extract and validate fixtures for the first table test parameter.
        let Some(first_param) = func.inputs.first() else {
            self.result.single_fail(Some("Table test should have at least one parameter".into()));
            return self.result;
        };

        let Some(first_param_fixtures) =
            &self.setup.fuzz_fixtures.param_fixtures(first_param.name())
        else {
            self.result.single_fail(Some("Table test should have fixtures defined".into()));
            return self.result;
        };

        if first_param_fixtures.is_empty() {
            self.result.single_fail(Some("Table test should have at least one fixture".into()));
            return self.result;
        }

        let fixtures_len = first_param_fixtures.len();
        let mut table_fixtures = vec![&first_param_fixtures[..]];

        // Collect fixtures for remaining parameters.
        for param in &func.inputs[1..] {
            let param_name = param.name();
            let Some(fixtures) = &self.setup.fuzz_fixtures.param_fixtures(param.name()) else {
                self.result.single_fail(Some(format!("No fixture defined for param {param_name}")));
                return self.result;
            };

            if fixtures.len() != fixtures_len {
                self.result.single_fail(Some(format!(
                    "{} fixtures defined for {param_name} (expected {})",
                    fixtures.len(),
                    fixtures_len
                )));
                return self.result;
            }

            table_fixtures.push(&fixtures[..]);
        }

        let progress = start_fuzz_progress(
            self.cr.progress,
            self.cr.name,
            &func.name,
            None,
            fixtures_len as u32,
        );

        for i in 0..fixtures_len {
            // Increment progress bar.
            if let Some(progress) = progress.as_ref() {
                progress.inc(1);
            }

            let args = table_fixtures.iter().map(|row| row[i].clone()).collect_vec();
            let (mut raw_call_result, reason) = match self.executor.call(
                self.sender,
                self.address,
                func,
                &args,
                U256::ZERO,
                Some(self.revert_decoder()),
            ) {
                Ok(res) => (res.raw, None),
                Err(EvmError::Execution(err)) => (err.raw, Some(err.reason)),
                Err(EvmError::Skip(reason)) => {
                    self.result.single_skip(reason);
                    return self.result;
                }
                Err(err) => {
                    self.result.single_fail(Some(err.to_string()));
                    return self.result;
                }
            };

            let is_success =
                self.executor.is_raw_call_mut_success(self.address, &mut raw_call_result, false);
            // Record counterexample if test fails.
            if !is_success {
                self.result.counterexample =
                    Some(CounterExample::Single(BaseCounterExample::from_fuzz_call(
                        Bytes::from(func.abi_encode_input(&args).unwrap()),
                        args,
                        raw_call_result.traces.clone(),
                    )));
                self.result.single_result(false, reason, raw_call_result);
                return self.result;
            }

            // If it's the last iteration and all other runs succeeded, then use last call result
            // for logs and traces.
            if i == fixtures_len - 1 {
                self.result.single_result(true, None, raw_call_result);
                return self.result;
            }
        }

        self.result
    }

    fn run_invariant_test(
        mut self,
        func: &Function,
        call_after_invariant: bool,
        identified_contracts: &ContractsByAddress,
        test_bytecode: &Bytes,
    ) -> TestResult {
        // First, run the test normally to see if it needs to be skipped.
        if let Err(EvmError::Skip(reason)) = self.executor.call(
            self.sender,
            self.address,
            func,
            &[],
            U256::ZERO,
            Some(self.revert_decoder()),
        ) {
            self.result.invariant_skip(reason);
            return self.result;
        };

        let runner = self.invariant_runner();
        let invariant_config = &self.config.invariant;

        let mut evm = InvariantExecutor::new(
            self.clone_executor(),
            runner,
            invariant_config.clone(),
            identified_contracts,
            &self.cr.mcr.known_contracts,
        );
        let invariant_contract = InvariantContract {
            address: self.address,
            invariant_function: func,
            call_after_invariant,
            abi: &self.cr.contract.abi,
        };

        let (failure_dir, failure_file) = invariant_failure_paths(
            invariant_config,
            self.cr.name,
            &invariant_contract.invariant_function.name,
        );
        let show_solidity = invariant_config.show_solidity;

        // Try to replay recorded failure if any.
        if let Some(mut call_sequence) =
            persisted_call_sequence(failure_file.as_path(), test_bytecode)
        {
            // Create calls from failed sequence and check if invariant still broken.
            let txes = call_sequence
                .iter_mut()
                .map(|seq| {
                    seq.show_solidity = show_solidity;
                    BasicTxDetails {
                        sender: seq.sender.unwrap_or_default(),
                        call_details: CallDetails {
                            target: seq.addr.unwrap_or_default(),
                            calldata: seq.calldata.clone(),
                        },
                    }
                })
                .collect::<Vec<BasicTxDetails>>();
            if let Ok((success, replayed_entirely)) = check_sequence(
                self.clone_executor(),
                &txes,
                (0..min(txes.len(), invariant_config.depth as usize)).collect(),
                invariant_contract.address,
                invariant_contract.invariant_function.selector().to_vec().into(),
                invariant_config.fail_on_revert,
                invariant_contract.call_after_invariant,
            ) && !success
            {
                let _ = sh_warn!(
                    "\
                            Replayed invariant failure from {:?} file. \
                            Run `forge clean` or remove file to ignore failure and to continue invariant test campaign.",
                    failure_file.as_path()
                );
                // If sequence still fails then replay error to collect traces and
                // exit without executing new runs.
                let _ = replay_run(
                    &invariant_contract,
                    self.clone_executor(),
                    &self.cr.mcr.known_contracts,
                    identified_contracts.clone(),
                    &mut self.result.logs,
                    &mut self.result.traces,
                    &mut self.result.line_coverage,
                    &mut self.result.deprecated_cheatcodes,
                    &txes,
                    show_solidity,
                );
                self.result.invariant_replay_fail(
                    replayed_entirely,
                    &invariant_contract.invariant_function.name,
                    call_sequence,
                );
                return self.result;
            }
        }

        let progress = start_fuzz_progress(
            self.cr.progress,
            self.cr.name,
            &func.name,
            invariant_config.timeout,
            invariant_config.runs,
        );
        let invariant_result = match evm.invariant_fuzz(
            invariant_contract.clone(),
            &self.setup.fuzz_fixtures,
            &self.setup.deployed_libs,
            progress.as_ref(),
        ) {
            Ok(x) => x,
            Err(e) => {
                self.result.invariant_setup_fail(e);
                return self.result;
            }
        };
        // Merge coverage collected during invariant run with test setup coverage.
        self.result.merge_coverages(invariant_result.line_coverage);

        let mut counterexample = None;
        let success = invariant_result.error.is_none();
        let reason = invariant_result.error.as_ref().and_then(|err| err.revert_reason());

        match invariant_result.error {
            // If invariants were broken, replay the error to collect logs and traces
            Some(error) => match error {
                InvariantFuzzError::BrokenInvariant(case_data)
                | InvariantFuzzError::Revert(case_data) => {
                    // Replay error to create counterexample and to collect logs, traces and
                    // coverage.
                    match replay_error(
                        &case_data,
                        &invariant_contract,
                        self.clone_executor(),
                        &self.cr.mcr.known_contracts,
                        identified_contracts.clone(),
                        &mut self.result.logs,
                        &mut self.result.traces,
                        &mut self.result.line_coverage,
                        &mut self.result.deprecated_cheatcodes,
                        progress.as_ref(),
                        show_solidity,
                    ) {
                        Ok(call_sequence) => {
                            if !call_sequence.is_empty() {
                                // Persist error in invariant failure dir.
                                if let Err(err) = foundry_common::fs::create_dir_all(failure_dir) {
                                    error!(%err, "Failed to create invariant failure dir");
                                } else if let Err(err) = foundry_common::fs::write_json_file(
                                    failure_file.as_path(),
                                    &InvariantPersistedFailure {
                                        call_sequence: call_sequence.clone(),
                                        driver_bytecode: Some(test_bytecode.clone()),
                                    },
                                ) {
                                    error!(%err, "Failed to record call sequence");
                                }

                                let original_seq_len =
                                    if let TestError::Fail(_, calls) = &case_data.test_error {
                                        calls.len()
                                    } else {
                                        call_sequence.len()
                                    };

                                counterexample =
                                    Some(CounterExample::Sequence(original_seq_len, call_sequence))
                            }
                        }
                        Err(err) => {
                            error!(%err, "Failed to replay invariant error");
                        }
                    };
                }
                InvariantFuzzError::MaxAssumeRejects(_) => {}
            },

            // If invariants ran successfully, replay the last run to collect logs and
            // traces.
            _ => {
                if let Err(err) = replay_run(
                    &invariant_contract,
                    self.clone_executor(),
                    &self.cr.mcr.known_contracts,
                    identified_contracts.clone(),
                    &mut self.result.logs,
                    &mut self.result.traces,
                    &mut self.result.line_coverage,
                    &mut self.result.deprecated_cheatcodes,
                    &invariant_result.last_run_inputs,
                    show_solidity,
                ) {
                    error!(%err, "Failed to replay last invariant run");
                }
            }
        }

        self.result.invariant_result(
            invariant_result.gas_report_traces,
            success,
            reason,
            counterexample,
            invariant_result.cases,
            invariant_result.reverts,
            invariant_result.metrics,
            invariant_result.failed_corpus_replays,
        );
        self.result
    }

    /// Runs a fuzzed test.
    ///
    /// Applies the before test txes (if any), fuzzes the current function and returns the
    /// `TestResult`.
    ///
    /// Before test txes are applied in order and state modifications committed to the EVM database
    /// (therefore the fuzz test will use the modified state).
    /// State modifications of before test txes and fuzz test are discarded after test ends,
    /// similar to `eth_call`.
    fn run_fuzz_test(mut self, func: &Function) -> TestResult {
        // Prepare fuzz test execution.
        if self.prepare_test(func).is_err() {
            return self.result;
        }

        let runner = self.fuzz_runner();
        let fuzz_config = self.config.fuzz.clone();

        let progress = start_fuzz_progress(
            self.cr.progress,
            self.cr.name,
            &func.name,
            fuzz_config.timeout,
            fuzz_config.runs,
        );

        // Run fuzz test.
        let fuzzed_executor =
            FuzzedExecutor::new(self.executor.into_owned(), runner, self.tcfg.sender, fuzz_config);
        let result = fuzzed_executor.fuzz(
            func,
            &self.setup.fuzz_fixtures,
            &self.setup.deployed_libs,
            self.address,
            &self.cr.mcr.revert_decoder,
            progress.as_ref(),
        );
        self.result.fuzz_result(result);
        self.result
    }

    /// Prepares single unit test and fuzz test execution:
    /// - set up the test result and executor
    /// - check if before test txes are configured and apply them in order
    ///
    /// Before test txes are arrays of arbitrary calldata obtained by calling the `beforeTest`
    /// function with test selector as a parameter.
    ///
    /// Unit tests within same contract (or even current test) are valid options for before test tx
    /// configuration. Test execution stops if any of before test txes fails.
    fn prepare_test(&mut self, func: &Function) -> Result<(), ()> {
        let address = self.setup.address;

        // Apply before test configured functions (if any).
        if self.cr.contract.abi.functions().filter(|func| func.name.is_before_test_setup()).count()
            == 1
        {
            for calldata in self.executor.call_sol_default(
                address,
                &ITest::beforeTestSetupCall { testSelector: func.selector() },
            ) {
                // Apply before test configured calldata.
                match self.executor.to_mut().transact_raw(
                    self.tcfg.sender,
                    address,
                    calldata,
                    U256::ZERO,
                ) {
                    Ok(call_result) => {
                        let reverted = call_result.reverted;

                        // Merge tx result traces in unit test result.
                        self.result.extend(call_result);

                        // To continue unit test execution the call should not revert.
                        if reverted {
                            self.result.single_fail(None);
                            return Err(());
                        }
                    }
                    Err(_) => {
                        self.result.single_fail(None);
                        return Err(());
                    }
                }
            }
        }
        Ok(())
    }

    fn fuzz_runner(&self) -> TestRunner {
        let config = &self.config.fuzz;
        let failure_persist_path = config
            .failure_persist_dir
            .as_ref()
            .unwrap()
            .join(config.failure_persist_file.as_ref().unwrap())
            .into_os_string()
            .into_string()
            .unwrap();
        fuzzer_with_cases(
            config.seed,
            config.runs,
            config.max_test_rejects,
            Some(Box::new(FileFailurePersistence::Direct(failure_persist_path.leak()))),
        )
    }

    fn invariant_runner(&self) -> TestRunner {
        let config = &self.config.invariant;
        fuzzer_with_cases(self.config.fuzz.seed, config.runs, config.max_assume_rejects, None)
    }

    fn clone_executor(&self) -> Executor {
        self.executor.clone().into_owned()
    }
}

fn fuzzer_with_cases(
    seed: Option<U256>,
    cases: u32,
    max_global_rejects: u32,
    file_failure_persistence: Option<Box<dyn FailurePersistence>>,
) -> TestRunner {
    let config = proptest::test_runner::Config {
        failure_persistence: file_failure_persistence,
        cases,
        max_global_rejects,
        // Disable proptest shrink: for fuzz tests we provide single counterexample,
        // for invariant tests we shrink outside proptest.
        max_shrink_iters: 0,
        ..Default::default()
    };

    if let Some(seed) = seed {
        trace!(target: "forge::test", %seed, "building deterministic fuzzer");
        let rng = TestRng::from_seed(RngAlgorithm::ChaCha, &seed.to_be_bytes::<32>());
        TestRunner::new_with_rng(config, rng)
    } else {
        trace!(target: "forge::test", "building stochastic fuzzer");
        TestRunner::new(config)
    }
}

/// Holds data about a persisted invariant failure.
#[derive(Serialize, Deserialize)]
struct InvariantPersistedFailure {
    /// Recorded counterexample.
    call_sequence: Vec<BaseCounterExample>,
    /// Bytecode of the test contract that generated the counterexample.
    #[serde(skip_serializing_if = "Option::is_none")]
    driver_bytecode: Option<Bytes>,
}

/// Helper function to load failed call sequence from file.
/// Ignores failure if generated with different test contract than the current one.
fn persisted_call_sequence(path: &Path, bytecode: &Bytes) -> Option<Vec<BaseCounterExample>> {
    foundry_common::fs::read_json_file::<InvariantPersistedFailure>(path).ok().and_then(
        |persisted_failure| {
            if let Some(persisted_bytecode) = &persisted_failure.driver_bytecode {
                // Ignore persisted sequence if test bytecode doesn't match.
                if !bytecode.eq(persisted_bytecode) {
                    let _= sh_warn!("\
                            Failure from {:?} file was ignored because test contract bytecode has changed.",
                        path
                    );
                    return None;
                }
            };
            Some(persisted_failure.call_sequence)
        },
    )
}

/// Helper functions to return canonicalized invariant failure paths.
fn invariant_failure_paths(
    config: &InvariantConfig,
    contract_name: &str,
    invariant_name: &str,
) -> (PathBuf, PathBuf) {
    let dir = config
        .failure_persist_dir
        .clone()
        .unwrap()
        .join("failures")
        .join(contract_name.split(':').next_back().unwrap());
    let dir = canonicalized(dir);
    let file = canonicalized(dir.join(invariant_name));
    (dir, file)
}
