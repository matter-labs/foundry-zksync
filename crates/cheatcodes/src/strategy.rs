use std::{any::Any, fmt::Debug, sync::Arc};

use alloy_consensus::BlobTransactionSidecar;
use alloy_network::TransactionBuilder4844;
use alloy_primitives::{Address, TxKind};
use alloy_rpc_types::{TransactionInput, TransactionRequest};
use revm::{
    context_interface::transaction::SignedAuthorization,
    interpreter::{CallInputs, CallOutcome, CreateOutcome, Interpreter},
};

use crate::{
    BroadcastableTransaction, BroadcastableTransactions, Cheatcodes, CheatcodesExecutor,
    CheatsConfig, CheatsCtxt, DynCheatcode, Result,
    inspector::{CommonCreateInput, Ecx, check_if_fixed_gas_limit},
    script::Broadcast,
};

/// Represents the context for [CheatcodeInspectorStrategy].
pub trait CheatcodeInspectorStrategyContext: Debug + Send + Sync + Any {
    /// Clone the strategy context.
    fn new_cloned(&self) -> Box<dyn CheatcodeInspectorStrategyContext>;
    /// Alias as immutable reference of [Any].
    fn as_any_ref(&self) -> &dyn Any;
    /// Alias as mutable reference of [Any].
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl CheatcodeInspectorStrategyContext for () {
    fn new_cloned(&self) -> Box<dyn CheatcodeInspectorStrategyContext> {
        Box::new(())
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }
}

/// Represents the strategy.
#[derive(Debug)]
pub struct CheatcodeInspectorStrategy {
    /// Strategy runner.
    pub runner: &'static dyn CheatcodeInspectorStrategyRunner,
    /// Strategy context.
    pub context: Box<dyn CheatcodeInspectorStrategyContext>,
}

impl CheatcodeInspectorStrategy {
    pub fn new_evm() -> Self {
        Self { runner: &EvmCheatcodeInspectorStrategyRunner, context: Box::new(()) }
    }
}

impl Clone for CheatcodeInspectorStrategy {
    fn clone(&self) -> Self {
        Self { runner: self.runner, context: self.context.new_cloned() }
    }
}

pub trait CheatcodeInspectorStrategyRunner:
    Debug + Send + Sync + CheatcodeInspectorStrategyExt
{
    fn apply_full(
        &self,
        cheatcode: &dyn DynCheatcode,
        ccx: &mut CheatsCtxt,
        executor: &mut dyn CheatcodesExecutor,
    ) -> Result {
        cheatcode.dyn_apply(ccx, executor)
    }

    /// Called when the main test or script contract is deployed.
    fn base_contract_deployed(&self, _ctx: &mut dyn CheatcodeInspectorStrategyContext) {}

    /// Record broadcastable transaction during CREATE.
    fn record_broadcastable_create_transactions(
        &self,
        _ctx: &mut dyn CheatcodeInspectorStrategyContext,
        config: Arc<CheatsConfig>,
        input: &dyn CommonCreateInput,
        ecx: Ecx,
        broadcast: &Broadcast,
        broadcastable_transactions: &mut BroadcastableTransactions,
    );

    /// Record broadcastable transaction during CALL.
    #[allow(clippy::too_many_arguments)]
    fn record_broadcastable_call_transactions(
        &self,
        _ctx: &mut dyn CheatcodeInspectorStrategyContext,
        config: Arc<CheatsConfig>,
        input: &CallInputs,
        ecx: Ecx,
        broadcast: &Broadcast,
        broadcastable_transactions: &mut BroadcastableTransactions,
        active_delegations: Vec<SignedAuthorization>,
        active_blob_sidecar: Option<BlobTransactionSidecar>,
    );

    fn post_initialize_interp(
        &self,
        _ctx: &mut dyn CheatcodeInspectorStrategyContext,
        _interpreter: &mut Interpreter,
        _ecx: Ecx,
    ) {
    }

    /// Used to override opcode behaviors. Returns true if handled.
    fn pre_step_end(
        &self,
        _ctx: &mut dyn CheatcodeInspectorStrategyContext,
        _interpreter: &mut Interpreter,
        _ecx: Ecx,
    ) -> bool {
        false
    }
}

/// We define this in our fork
pub trait CheatcodeInspectorStrategyExt {
    fn zksync_record_create_address(
        &self,
        _ctx: &mut dyn CheatcodeInspectorStrategyContext,
        _outcome: &CreateOutcome,
    ) {
    }

    fn zksync_sync_nonce(
        &self,
        _ctx: &mut dyn CheatcodeInspectorStrategyContext,
        _sender: Address,
        _nonce: u64,
        _ecx: Ecx,
    ) {
    }

    fn zksync_set_deployer_call_input(
        &self,
        _ecx: Ecx,
        _ctx: &mut dyn CheatcodeInspectorStrategyContext,
        _call: &mut CallInputs,
    ) {
    }

    fn zksync_try_create(
        &self,
        _state: &mut Cheatcodes,
        _ecx: Ecx,
        _input: &dyn CommonCreateInput,
        _executor: &mut dyn CheatcodesExecutor,
    ) -> Option<CreateOutcome> {
        None
    }

    fn zksync_try_call(
        &self,
        _state: &mut Cheatcodes,
        _ecx: Ecx,
        _input: &CallInputs,
        _executor: &mut dyn CheatcodesExecutor,
    ) -> Option<CallOutcome> {
        None
    }

    fn zksync_remove_duplicate_account_access(&self, _state: &mut Cheatcodes) {}

    fn zksync_increment_nonce_after_broadcast(
        &self,
        _state: &mut Cheatcodes,
        _ecx: Ecx,
        _is_static: bool,
    ) {
    }
}

#[derive(Debug, Default, Clone)]
pub struct EvmCheatcodeInspectorStrategyRunner;

impl CheatcodeInspectorStrategyRunner for EvmCheatcodeInspectorStrategyRunner {
    fn record_broadcastable_create_transactions(
        &self,
        _ctx: &mut dyn CheatcodeInspectorStrategyContext,
        _config: Arc<CheatsConfig>,
        input: &dyn CommonCreateInput,
        ecx: Ecx,
        broadcast: &Broadcast,
        broadcastable_transactions: &mut BroadcastableTransactions,
    ) {
        let is_fixed_gas_limit = check_if_fixed_gas_limit(&ecx, input.gas_limit());

        let to = None;
        let nonce: u64 = ecx.journaled_state.state()[&broadcast.new_origin].info.nonce;
        //drop the mutable borrow of account
        let call_init_code = input.init_code();
        let rpc = ecx.journaled_state.database.active_fork_url();

        broadcastable_transactions.push_back(BroadcastableTransaction {
            rpc,
            transaction: TransactionRequest {
                from: Some(broadcast.new_origin),
                to,
                value: Some(input.value()),
                input: TransactionInput::new(call_init_code),
                nonce: Some(nonce),
                gas: if is_fixed_gas_limit { Some(input.gas_limit()) } else { None },
                ..Default::default()
            }
            .into(),
        });
    }

    fn record_broadcastable_call_transactions(
        &self,
        _ctx: &mut dyn CheatcodeInspectorStrategyContext,
        _config: Arc<CheatsConfig>,
        call: &CallInputs,
        ecx: Ecx,
        broadcast: &Broadcast,
        broadcastable_transactions: &mut BroadcastableTransactions,
        active_delegation: Vec<SignedAuthorization>,
        active_blob_sidecar: Option<BlobTransactionSidecar>,
    ) {
        let input = TransactionInput::new(call.input.bytes(ecx));
        let is_fixed_gas_limit = check_if_fixed_gas_limit(&ecx, call.gas_limit);

        let account = ecx.journaled_state.state().get_mut(&broadcast.new_origin).unwrap();
        let nonce = account.info.nonce;

        let mut tx_req = TransactionRequest {
            from: Some(broadcast.new_origin),
            to: Some(TxKind::from(Some(call.target_address))),
            value: call.transfer_value(),
            input,
            nonce: Some(nonce),
            chain_id: Some(ecx.cfg.chain_id),
            gas: if is_fixed_gas_limit { Some(call.gas_limit) } else { None },
            ..Default::default()
        };

        match (active_delegation.is_empty(), active_blob_sidecar) {
            (false, Some(_)) => {
                // Note(zk): We can't return a call outcome from here
                return;
            }
            (false, None) => {
                tx_req.authorization_list = Some(active_delegation);
                tx_req.sidecar = None;

                // Increment nonce to reflect the signed authorization.
                account.info.nonce += 1;
            }
            (true, Some(blob_sidecar)) => {
                tx_req.set_blob_sidecar(blob_sidecar);
                tx_req.authorization_list = None;
            }
            (true, None) => {
                tx_req.sidecar = None;
                tx_req.authorization_list = None;
            }
        }

        broadcastable_transactions.push_back(BroadcastableTransaction {
            rpc: ecx.journaled_state.database.active_fork_url(),
            transaction: tx_req.into(),
        });
        debug!(target: "cheatcodes", tx=?broadcastable_transactions.back().unwrap(), "broadcastable call");
    }
}

impl CheatcodeInspectorStrategyExt for EvmCheatcodeInspectorStrategyRunner {}

struct _ObjectSafe0(dyn CheatcodeInspectorStrategyRunner);
struct _ObjectSafe1(dyn CheatcodeInspectorStrategyExt);
