use alloy_dyn_abi::{DynSolValue, FunctionExt, JsonAbiExt};
use alloy_json_abi::{Function, JsonAbi};
use foundry_common::abi::IntoFunction;
use foundry_evm_core::{
    backend::{Backend, DatabaseError, DatabaseExt, DatabaseResult, FuzzBackendWrapper},
    constants::{
        CALLER, CHEATCODE_ADDRESS, DEFAULT_CREATE2_DEPLOYER, DEFAULT_CREATE2_DEPLOYER_CODE,
    },
    decode,
    utils::{eval_to_instruction_result, halt_to_instruction_result, StateChangeset},
};
use revm::{
    interpreter::{return_ok, InstructionResult},
    primitives::{
        Address, BlockEnv, Bytes, CreateScheme, Env, ExecutionResult, Output, ResultAndState,
        SpecId, TransactTo, TxEnv, U256,
    },
    Database, DatabaseCommit, DatabaseRef,
};
use tracing::trace;

use super::inspector::NoopInspector;

/// A type that can execute calls
///
/// Simplified version of what's present in `foundry_evm`
#[derive(Debug, Clone)]
pub struct Executor {
    /// The underlying `revm::Database` that contains the EVM storage.
    pub backend: Backend,
    /// The EVM environment.
    pub env: Env,
    /// The gas limit for calls and deployments. This is different from the gas limit imposed by
    /// the passed in environment, as those limits are used by the EVM for certain opcodes like
    /// `gaslimit`.
    gas_limit: U256,
    //we hold an inspector so it has &'self lifetime
    inspector: NoopInspector,
}

impl Executor {
    #[inline]
    //no InspectorStack -> circular dependency
    pub fn new(backend: Backend, env: Env, gas_limit: U256) -> Self {
        Executor { backend, env, gas_limit, inspector: NoopInspector }
    }

    /// Creates the default CREATE2 Contract Deployer for local tests and scripts.
    pub fn deploy_create2_deployer(&mut self) -> eyre::Result<()> {
        trace!("deploying local create2 deployer");
        let create2_deployer_account = self
            .backend
            .basic(DEFAULT_CREATE2_DEPLOYER)?
            .ok_or(DatabaseError::MissingAccount(DEFAULT_CREATE2_DEPLOYER))?;

        // if the deployer is not currently deployed, deploy the default one
        if create2_deployer_account.code.map_or(true, |code| code.is_empty()) {
            let creator = "0x3fAB184622Dc19b6109349B94811493BF2a45362".parse().unwrap();

            // Probably 0, but just in case.
            let initial_balance = self.get_balance(creator)?;

            self.set_balance(creator, U256::MAX)?;
            let res =
                self.deploy(creator, DEFAULT_CREATE2_DEPLOYER_CODE.into(), U256::ZERO, None)?;
            trace!(create2=?res.address, "deployed local create2 deployer");

            self.set_balance(creator, initial_balance)?;
        }
        Ok(())
    }

    /// Set the balance of an account.
    pub fn set_balance(&mut self, address: Address, amount: U256) -> DatabaseResult<&mut Self> {
        trace!(?address, ?amount, "setting account balance");
        let mut account = self.backend.basic(address)?.unwrap_or_default();
        account.balance = amount;

        self.backend.insert_account_info(address, account);
        Ok(self)
    }

    /// Gets the balance of an account
    pub fn get_balance(&self, address: Address) -> DatabaseResult<U256> {
        Ok(self.backend.basic_ref(address)?.map(|acc| acc.balance).unwrap_or_default())
    }

    /// Set the nonce of an account.
    pub fn set_nonce(&mut self, address: Address, nonce: u64) -> DatabaseResult<&mut Self> {
        let mut account = self.backend.basic(address)?.unwrap_or_default();
        account.nonce = nonce;

        self.backend.insert_account_info(address, account);
        Ok(self)
    }

    #[inline]
    pub fn set_gas_limit(&mut self, gas_limit: U256) -> &mut Self {
        self.gas_limit = gas_limit;
        self
    }

    /// Performs a call to an account on the current state of the VM.
    ///
    /// The state after the call is persisted.
    pub fn call_committing<T: Into<Vec<DynSolValue>>, F: IntoFunction>(
        &mut self,
        from: Address,
        to: Address,
        func: F,
        args: T,
        value: U256,
        abi: Option<&JsonAbi>,
    ) -> Result<CallResult, EvmError> {
        let func = func.into();
        let calldata = Bytes::from(func.abi_encode_input(&args.into())?.to_vec());
        let result = self.call_raw_committing(from, to, calldata, value)?;
        convert_call_result(abi, &func, result)
    }

    /// Performs a raw call to an account on the current state of the VM.
    ///
    /// The state after the call is persisted.
    pub fn call_raw_committing(
        &mut self,
        from: Address,
        to: Address,
        calldata: Bytes,
        value: U256,
    ) -> eyre::Result<RawCallResult> {
        tracing::info!(?from, ?to, ?calldata, ?value, "Calling EVM");
        let env = self.build_test_env(from, TransactTo::Call(to), calldata, value);
        let mut result = self.call_raw_with_env(env)?;
        tracing::info!(result = ?result.out, "Calling EVM, result");
        self.commit(&mut result);
        Ok(result)
    }

    /// Performs a call to an account on the current state of the VM.
    ///
    /// The state after the call is not persisted.
    pub fn call<T: Into<Vec<DynSolValue>>, F: IntoFunction>(
        &self,
        from: Address,
        to: Address,
        func: F,
        args: T,
        value: U256,
        abi: Option<&JsonAbi>,
    ) -> Result<CallResult, EvmError> {
        let func = func.into();
        let calldata = Bytes::from(func.abi_encode_input(&args.into())?.to_vec());
        let call_result = self.call_raw(from, to, calldata, value)?;
        convert_call_result(abi, &func, call_result)
    }

    /// Performs a raw call to an account on the current state of the VM.
    ///
    /// Any state modifications made by the call are not committed.
    pub fn call_raw(
        &self,
        from: Address,
        to: Address,
        calldata: Bytes,
        value: U256,
    ) -> eyre::Result<RawCallResult> {
        let mut inspector = self.inspector.clone();
        // Build VM
        let mut env = self.build_test_env(from, TransactTo::Call(to), calldata, value);

        let mut db = FuzzBackendWrapper::new(&self.backend);
        let result = db.inspect_ref_EVM(&mut env, &mut inspector)?;

        // Persist the snapshot failure recorded on the fuzz backend wrapper.
        let has_snapshot_failure = db.has_snapshot_failure();
        convert_executed_result(env, result, has_snapshot_failure)
    }

    /// Execute the transaction configured in `env.tx` and commit the changes
    pub fn commit_tx_with_env(&mut self, env: Env) -> eyre::Result<RawCallResult> {
        let mut result = self.call_raw_with_env(env)?;
        self.commit(&mut result);
        Ok(result)
    }

    /// Execute the transaction configured in `env.tx`
    pub fn call_raw_with_env(&mut self, mut env: Env) -> eyre::Result<RawCallResult> {
        // execute the call
        let mut inspector = self.inspector.clone();
        let result = self.backend.inspect_ref_EVM(&mut env, &mut inspector)?;
        convert_executed_result(env, result, self.backend.has_snapshot_failure())
    }

    /// Commit the changeset to the database and adjust `self.inspector_config`
    /// values according to the executed call result
    fn commit(&mut self, result: &mut RawCallResult) {
        // Persist changes to db
        if let Some(changes) = &result.state_changeset {
            self.backend.commit(changes.clone());
        }
    }

    /// Deploys a contract using the given `env` and commits the new state to the underlying
    /// database
    pub fn deploy_with_env(
        &mut self,
        env: Env,
        abi: Option<&JsonAbi>,
    ) -> Result<DeployResult, EvmError> {
        debug_assert!(
            matches!(env.tx.transact_to, TransactTo::Create(_)),
            "Expect create transaction"
        );
        trace!(sender=?env.tx.caller, "deploying contract");

        let mut result = self.call_raw_with_env(env)?;
        self.commit(&mut result);

        let RawCallResult { exit_reason, out, gas_used, gas_refunded, env, .. } = result;

        let result = match &out {
            Some(Output::Create(data, _)) => data.to_owned(),
            _ => Bytes::default(),
        };

        let address = match exit_reason {
            return_ok!() => {
                if let Some(Output::Create(_, Some(addr))) = out {
                    addr
                } else {
                    return Err(EvmError::Execution(Box::new(ExecutionErr {
                        reverted: true,
                        reason: "Deployment succeeded, but no address was returned. This is a bug, please report it".to_string(),
                        gas_used,
                        gas_refunded: 0,
                        stipend: 0,
                        state_changeset: None,
                    })));
                }
            }
            _ => {
                let reason = decode::decode_revert(result.as_ref(), abi, Some(exit_reason));
                return Err(EvmError::Execution(Box::new(ExecutionErr {
                    reverted: true,
                    reason,
                    gas_used,
                    gas_refunded,
                    stipend: 0,
                    state_changeset: None,
                })))
            }
        };

        // also mark this library as persistent, this will ensure that the state of the library is
        // persistent across fork swaps in forking mode
        self.backend.add_persistent_account(address);

        trace!(address=?address, "deployed contract");

        Ok(DeployResult { address, gas_used, gas_refunded, env })
    }

    /// Deploys a contract and commits the new state to the underlying database.
    ///
    /// Executes a CREATE transaction with the contract `code` and persistent database state
    /// modifications
    pub fn deploy(
        &mut self,
        from: Address,
        code: Bytes,
        value: U256,
        abi: Option<&JsonAbi>,
    ) -> Result<DeployResult, EvmError> {
        let env = self.build_test_env(from, TransactTo::Create(CreateScheme::Create), code, value);
        self.deploy_with_env(env, abi)
    }

    /// Check if a call to a test contract was successful.
    ///
    /// This function checks both the VM status of the call, DSTest's `failed` status and the
    /// `globalFailed` flag which is stored in `failed` inside the `CHEATCODE_ADDRESS` contract.
    ///
    /// DSTest will not revert inside its `assertEq`-like functions which allows
    /// to test multiple assertions in 1 test function while also preserving logs.
    ///
    /// If an `assert` is violated, the contract's `failed` variable is set to true, and the
    /// `globalFailure` flag inside the `CHEATCODE_ADDRESS` is also set to true, this way, failing
    /// asserts from any contract are tracked as well.
    ///
    /// In order to check whether a test failed, we therefore need to evaluate the contract's
    /// `failed` variable and the `globalFailure` flag, which happens by calling
    /// `contract.failed()`.
    pub fn is_success(
        &self,
        address: Address,
        reverted: bool,
        state_changeset: StateChangeset,
        should_fail: bool,
    ) -> bool {
        self.ensure_success(address, reverted, state_changeset, should_fail).unwrap_or_default()
    }

    fn ensure_success(
        &self,
        address: Address,
        reverted: bool,
        state_changeset: StateChangeset,
        should_fail: bool,
    ) -> Result<bool, DatabaseError> {
        if self.backend.has_snapshot_failure() {
            // a failure occurred in a reverted snapshot, which is considered a failed test
            return Ok(should_fail)
        }

        // Construct a new VM with the state changeset
        let mut backend = self.backend.clone_empty();

        // we only clone the test contract and cheatcode accounts, that's all we need to evaluate
        // success
        for addr in [address, CHEATCODE_ADDRESS] {
            let acc = self.backend.basic_ref(addr)?.unwrap_or_default();
            backend.insert_account_info(addr, acc);
        }

        // If this test failed any asserts, then this changeset will contain changes `false -> true`
        // for the contract's `failed` variable and the `globalFailure` flag in the state of the
        // cheatcode address which are both read when call `"failed()(bool)"` in the next step
        backend.commit(state_changeset);
        let executor = Executor::new(backend, self.env.clone(), self.gas_limit);

        let mut success = !reverted;
        if success {
            // Check if a DSTest assertion failed
            let call = executor.call(CALLER, address, "failed()(bool)", vec![], U256::ZERO, None);

            if let Ok(CallResult { result: failed, .. }) = call {
                success = !failed.as_bool().unwrap();
            }
        }

        Ok(should_fail ^ success)
    }

    /// Creates the environment to use when executing a transaction in a test context
    ///
    /// If using a backend with cheatcodes, `tx.gas_price` and `block.number` will be overwritten by
    /// the cheatcode state inbetween calls.
    fn build_test_env(
        &self,
        caller: Address,
        transact_to: TransactTo,
        data: Bytes,
        value: U256,
    ) -> Env {
        Env {
            cfg: self.env.cfg.clone(),
            // We always set the gas price to 0 so we can execute the transaction regardless of
            // network conditions - the actual gas price is kept in `self.block` and is applied by
            // the cheatcode handler if it is enabled
            block: BlockEnv {
                basefee: U256::from(0),
                gas_limit: self.gas_limit,
                ..self.env.block.clone()
            },
            tx: TxEnv {
                caller,
                transact_to,
                data,
                value,
                // As above, we set the gas price to 0.
                gas_price: U256::from(0),
                gas_priority_fee: None,
                gas_limit: self.gas_limit.to(),
                ..self.env.tx.clone()
            },
        }
    }
}

/// Represents the context after an execution error occurred.
#[derive(thiserror::Error, Debug)]
#[error("Execution reverted: {reason} (gas: {gas_used})")]
pub struct ExecutionErr {
    pub reverted: bool,
    pub reason: String,
    pub gas_used: u64,
    pub gas_refunded: u64,
    pub stipend: u64,
    pub state_changeset: Option<StateChangeset>,
}

#[derive(thiserror::Error, Debug)]
pub enum EvmError {
    /// Error which occurred during execution of a transaction
    #[error(transparent)]
    Execution(Box<ExecutionErr>),
    /// Error which occurred during ABI encoding/decoding
    #[error(transparent)]
    AbiError(#[from] alloy_dyn_abi::Error),
    /// Any other error.
    #[error(transparent)]
    Eyre(#[from] eyre::Error),
}

/// The result of a deployment.
#[derive(Debug)]
pub struct DeployResult {
    /// The address of the deployed contract
    pub address: Address,
    /// The gas cost of the deployment
    pub gas_used: u64,
    /// The refunded gas
    pub gas_refunded: u64,
    /// The `revm::Env` after deployment
    pub env: Env,
}

/// The result of a call.
#[derive(Debug)]
pub struct CallResult {
    pub skipped: bool,
    /// Whether the call reverted or not
    pub reverted: bool,
    /// The decoded result of the call
    pub result: DynSolValue,
    /// The gas used for the call
    pub gas_used: u64,
    /// The refunded gas for the call
    pub gas_refunded: u64,
    /// The initial gas stipend for the transaction
    pub stipend: u64,
    /// The changeset of the state.
    ///
    /// This is only present if the changed state was not committed to the database (i.e. if you
    /// used `call` and `call_raw` not `call_committing` or `call_raw_committing`).
    pub state_changeset: Option<StateChangeset>,
    /// The `revm::Env` after the call
    pub env: Env,
}

/// The result of a raw call.
#[derive(Debug)]
pub struct RawCallResult {
    /// The status of the call
    pub exit_reason: InstructionResult,
    /// Whether the call reverted or not
    pub reverted: bool,
    /// The raw result of the call
    pub result: Bytes,
    /// The gas used for the call
    pub gas_used: u64,
    /// Refunded gas
    pub gas_refunded: u64,
    /// The initial gas stipend for the transaction
    pub stipend: u64,
    /// The labels assigned to addresses during the call
    /// The changeset of the state.
    ///
    /// This is only present if the changed state was not committed to the database (i.e. if you
    /// used `call` and `call_raw` not `call_committing` or `call_raw_committing`).
    pub state_changeset: Option<StateChangeset>,
    /// The `revm::Env` after the call
    pub env: Env,
    /// The raw output of the execution
    pub out: Option<Output>,
}

impl Default for RawCallResult {
    fn default() -> Self {
        Self {
            exit_reason: InstructionResult::Continue,
            reverted: false,
            result: Bytes::new(),
            gas_used: 0,
            gas_refunded: 0,
            stipend: 0,
            state_changeset: None,
            env: Default::default(),
            out: None,
        }
    }
}

/// Converts the data aggregated in the `inspector` and `call` to a `RawCallResult`
fn convert_executed_result(
    env: Env,
    result: ResultAndState,
    has_snapshot_failure: bool,
) -> eyre::Result<RawCallResult> {
    let ResultAndState { result: exec_result, state: state_changeset } = result;
    let (exit_reason, gas_refunded, gas_used, out) = match exec_result {
        ExecutionResult::Success { reason, gas_used, gas_refunded, output, .. } => {
            (eval_to_instruction_result(reason), gas_refunded, gas_used, Some(output))
        }
        ExecutionResult::Revert { gas_used, output } => {
            // Need to fetch the unused gas
            (InstructionResult::Revert, 0_u64, gas_used, Some(Output::Call(output)))
        }
        ExecutionResult::Halt { reason, gas_used } => {
            (halt_to_instruction_result(reason), 0_u64, gas_used, None)
        }
    };
    let stipend = calc_stipend(&env.tx.data, env.cfg.spec_id);

    let result = match out {
        Some(Output::Call(ref data)) => data.to_owned(),
        Some(Output::Create(ref data, _)) => data.to_owned(),
        _ => Bytes::default(),
    };

    Ok(RawCallResult {
        exit_reason,
        reverted: !matches!(exit_reason, return_ok!()),
        result,
        gas_used,
        gas_refunded,
        stipend,
        state_changeset: Some(state_changeset),
        env,
        out,
    })
}

/// Calculates the initial gas stipend for a transaction
fn calc_stipend(calldata: &[u8], spec: SpecId) -> u64 {
    let non_zero_data_cost = if SpecId::enabled(spec, SpecId::ISTANBUL) { 16 } else { 68 };
    calldata.iter().fold(21000, |sum, byte| sum + if *byte == 0 { 4 } else { non_zero_data_cost })
}

fn convert_call_result(
    abi: Option<&JsonAbi>,
    func: &Function,
    call_result: RawCallResult,
) -> Result<CallResult, EvmError> {
    let RawCallResult {
        result,
        exit_reason: status,
        reverted,
        gas_used,
        gas_refunded,
        stipend,
        state_changeset,
        env,
        ..
    } = call_result;

    match status {
        return_ok!() => {
            let mut result = func.abi_decode_output(&result, false)?;
            let res = if result.len() == 1 {
                result.pop().unwrap()
            } else {
                // combine results into a tuple
                DynSolValue::Tuple(result)
            };
            Ok(CallResult {
                reverted,
                result: res,
                gas_used,
                gas_refunded,
                stipend,
                state_changeset,
                env,
                skipped: false,
            })
        }
        _ => {
            let reason = decode::decode_revert(&result, abi, Some(status));
            Err(EvmError::Execution(Box::new(ExecutionErr {
                reverted,
                reason,
                gas_used,
                gas_refunded,
                stipend,
                state_changeset,
            })))
        }
    }
}
