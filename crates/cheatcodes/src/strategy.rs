use std::{
    fmt::Debug,
    sync::{Arc, Mutex},
};

use alloy_primitives::{Address, Bytes, FixedBytes, TxKind, U256};
use alloy_rpc_types::{TransactionInput, TransactionRequest};
use alloy_sol_types::SolValue;
use foundry_evm_core::backend::LocalForkId;
use revm::{
    interpreter::{CallInputs, CallOutcome, CreateOutcome, InstructionResult, Interpreter},
    primitives::{Bytecode, SignedAuthorization, KECCAK_EMPTY},
};

use crate::{
    evm::{self, journaled_account, mock::make_acc_non_empty, DealRecord},
    inspector::{check_if_fixed_gas_limit, CommonCreateInput, Ecx, InnerEcx},
    mock_call,
    script::Broadcast,
    BroadcastableTransaction, BroadcastableTransactions, Cheatcodes, CheatcodesExecutor,
    CheatsConfig, CheatsCtxt, Result,
};

pub trait CheatcodeInspectorStrategy: Debug + Send + Sync {
    fn name(&self) -> &'static str;

    fn new_cloned(&self) -> Arc<Mutex<dyn CheatcodeInspectorStrategy>>;
    /// Get nonce.
    fn get_nonce(&mut self, ccx: &mut CheatsCtxt, address: Address) -> Result<u64> {
        let account = ccx.ecx.journaled_state.load_account(address, &mut ccx.ecx.db)?;
        Ok(account.info.nonce)
    }

    /// Called when the main test or script contract is deployed.
    fn base_contract_deployed(&mut self) {}

    /// Cheatcode: roll.
    fn cheatcode_roll(&mut self, ccx: &mut CheatsCtxt, new_height: U256) -> Result {
        ccx.ecx.env.block.number = new_height;
        Ok(Default::default())
    }

    /// Cheatcode: warp.
    fn cheatcode_warp(&mut self, ccx: &mut CheatsCtxt, new_timestamp: U256) -> Result {
        ccx.ecx.env.block.timestamp = new_timestamp;
        Ok(Default::default())
    }

    /// Cheatcode: deal.
    fn cheatcode_deal(
        &mut self,
        ccx: &mut CheatsCtxt,
        address: Address,
        new_balance: U256,
    ) -> Result {
        let account = journaled_account(ccx.ecx, address)?;
        let old_balance = std::mem::replace(&mut account.info.balance, new_balance);
        let record = DealRecord { address, old_balance, new_balance };
        ccx.state.eth_deals.push(record);
        Ok(Default::default())
    }

    /// Cheatcode: etch.
    fn cheatcode_etch(
        &mut self,
        ccx: &mut CheatsCtxt,
        target: Address,
        new_runtime_bytecode: &Bytes,
    ) -> Result {
        ensure_not_precompile!(&target, ccx);
        ccx.ecx.load_account(target)?;
        let bytecode = Bytecode::new_raw(Bytes::copy_from_slice(new_runtime_bytecode));
        ccx.ecx.journaled_state.set_code(target, bytecode);
        Ok(Default::default())
    }

    /// Cheatcode: getNonce.
    fn cheatcode_get_nonce(&mut self, ccx: &mut CheatsCtxt, address: Address) -> Result {
        evm::get_nonce(ccx, &address)
    }

    /// Cheatcode: resetNonce.
    fn cheatcode_reset_nonce(&mut self, ccx: &mut CheatsCtxt, account: Address) -> Result {
        let account = journaled_account(ccx.ecx, account)?;
        // Per EIP-161, EOA nonces start at 0, but contract nonces
        // start at 1. Comparing by code_hash instead of code
        // to avoid hitting the case where account's code is None.
        let empty = account.info.code_hash == KECCAK_EMPTY;
        let nonce = if empty { 0 } else { 1 };
        account.info.nonce = nonce;
        debug!(target: "cheatcodes", nonce, "reset");
        Ok(Default::default())
    }

    /// Cheatcode: setNonce.
    fn cheatcode_set_nonce(
        &mut self,
        ccx: &mut CheatsCtxt,
        account: Address,
        new_nonce: u64,
    ) -> Result {
        let account = journaled_account(ccx.ecx, account)?;
        // nonce must increment only
        let current = account.info.nonce;
        ensure!(
            new_nonce >= current,
            "new nonce ({new_nonce}) must be strictly equal to or higher than the \
             account's current nonce ({current})"
        );
        account.info.nonce = new_nonce;
        Ok(Default::default())
    }

    /// Cheatcode: setNonceUnsafe.
    fn cheatcode_set_nonce_unsafe(
        &mut self,
        ccx: &mut CheatsCtxt,
        account: Address,
        new_nonce: u64,
    ) -> Result {
        let account = journaled_account(ccx.ecx, account)?;
        account.info.nonce = new_nonce;
        Ok(Default::default())
    }

    /// Mocks a call to return with a value.
    fn cheatcode_mock_call(
        &mut self,
        ccx: &mut CheatsCtxt,
        callee: Address,
        data: &Bytes,
        return_data: &Bytes,
    ) -> Result {
        let _ = make_acc_non_empty(&callee, ccx.ecx)?;
        mock_call(ccx.state, &callee, data, None, return_data, InstructionResult::Return);
        Ok(Default::default())
    }

    /// Mocks a call to revert with a value.
    fn cheatcode_mock_call_revert(
        &mut self,
        ccx: &mut CheatsCtxt,
        callee: Address,
        data: &Bytes,
        revert_data: &Bytes,
    ) -> Result {
        let _ = make_acc_non_empty(&callee, ccx.ecx)?;
        mock_call(ccx.state, &callee, data, None, revert_data, InstructionResult::Revert);
        Ok(Default::default())
    }

    /// Retrieve artifact code.
    fn get_artifact_code(&self, state: &Cheatcodes, path: &str, deployed: bool) -> Result {
        Ok(crate::fs::get_artifact_code(state, path, deployed)?.abi_encode())
    }

    /// Record broadcastable transaction during CREATE.
    fn record_broadcastable_create_transactions(
        &mut self,
        config: Arc<CheatsConfig>,
        input: &dyn CommonCreateInput,
        ecx_inner: InnerEcx,
        broadcast: &Broadcast,
        broadcastable_transactions: &mut BroadcastableTransactions,
    );

    /// Record broadcastable transaction during CALL.
    fn record_broadcastable_call_transactions(
        &mut self,
        config: Arc<CheatsConfig>,
        input: &CallInputs,
        ecx_inner: InnerEcx,
        broadcast: &Broadcast,
        broadcastable_transactions: &mut BroadcastableTransactions,
        active_delegation: &mut Option<SignedAuthorization>,
    );

    fn post_initialize_interp(&mut self, _interpreter: &mut Interpreter, _ecx: Ecx) {}

    /// Used to override opcode behaviors. Returns true if handled.
    fn pre_step_end(&mut self, _interpreter: &mut Interpreter, _ecx: Ecx) -> bool {
        false
    }
}

/// We define this in our fork
pub trait CheatcodeInspectorStrategyExt: CheatcodeInspectorStrategy {
    fn new_cloned_ext(&self) -> Arc<Mutex<dyn CheatcodeInspectorStrategyExt>>;

    fn zksync_cheatcode_skip_zkvm(&mut self) -> Result {
        Ok(Default::default())
    }

    fn zksync_cheatcode_set_paymaster(
        &mut self,
        _paymaster_address: Address,
        _paymaster_input: &Bytes,
    ) -> Result {
        Ok(Default::default())
    }

    fn zksync_cheatcode_use_factory_deps(&mut self, _name: String) -> Result {
        Ok(Default::default())
    }

    #[allow(clippy::too_many_arguments)]
    fn zksync_cheatcode_register_contract(
        &mut self,
        _name: String,
        _zk_bytecode_hash: FixedBytes<32>,
        _zk_deployed_bytecode: Vec<u8>,
        _zk_factory_deps: Vec<Vec<u8>>,
        _evm_bytecode_hash: FixedBytes<32>,
        _evm_deployed_bytecode: Vec<u8>,
        _evm_bytecode: Vec<u8>,
    ) -> Result {
        Ok(Default::default())
    }

    fn zksync_cheatcode_select_zk_vm(&mut self, _data: InnerEcx, _enable: bool) {}

    fn zksync_record_create_address(&mut self, _outcome: &CreateOutcome) {}

    fn zksync_sync_nonce(&mut self, _sender: Address, _nonce: u64, _ecx: Ecx) {}

    fn zksync_set_deployer_call_input(&mut self, _call: &mut CallInputs) {}

    fn zksync_try_create(
        &mut self,
        _state: &mut Cheatcodes,
        _ecx: Ecx,
        _input: &dyn CommonCreateInput,
        _executor: &mut dyn CheatcodesExecutor,
    ) -> Option<CreateOutcome> {
        None
    }

    fn zksync_try_call(
        &mut self,
        _state: &mut Cheatcodes,
        _ecx: Ecx,
        _input: &CallInputs,
        _executor: &mut dyn CheatcodesExecutor,
    ) -> Option<CallOutcome> {
        None
    }

    fn zksync_select_fork_vm(&mut self, _data: InnerEcx, _fork_id: LocalForkId) {}
}

#[derive(Debug, Default, Clone)]
pub struct EvmCheatcodeInspectorStrategy {}

impl CheatcodeInspectorStrategy for EvmCheatcodeInspectorStrategy {
    fn name(&self) -> &'static str {
        "evm"
    }

    fn new_cloned(&self) -> Arc<Mutex<dyn CheatcodeInspectorStrategy>> {
        Arc::new(Mutex::new(self.clone()))
    }

    fn record_broadcastable_create_transactions(
        &mut self,
        _config: Arc<CheatsConfig>,
        input: &dyn CommonCreateInput,
        ecx_inner: InnerEcx,
        broadcast: &Broadcast,
        broadcastable_transactions: &mut BroadcastableTransactions,
    ) {
        let is_fixed_gas_limit = check_if_fixed_gas_limit(ecx_inner, input.gas_limit());

        let to = None;
        let nonce: u64 = ecx_inner.journaled_state.state()[&broadcast.new_origin].info.nonce;
        //drop the mutable borrow of account
        let call_init_code = input.init_code();
        let rpc = ecx_inner.db.active_fork_url();

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
        &mut self,
        _config: Arc<CheatsConfig>,
        call: &CallInputs,
        ecx_inner: InnerEcx,
        broadcast: &Broadcast,
        broadcastable_transactions: &mut BroadcastableTransactions,
        active_delegation: &mut Option<SignedAuthorization>,
    ) {
        let is_fixed_gas_limit = check_if_fixed_gas_limit(ecx_inner, call.gas_limit);

        let account = ecx_inner.journaled_state.state().get_mut(&broadcast.new_origin).unwrap();
        let nonce = account.info.nonce;

        let mut tx_req = TransactionRequest {
            from: Some(broadcast.new_origin),
            to: Some(TxKind::from(Some(call.target_address))),
            value: call.transfer_value(),
            input: TransactionInput::new(call.input.clone()),
            nonce: Some(nonce),
            chain_id: Some(ecx_inner.env.cfg.chain_id),
            gas: if is_fixed_gas_limit { Some(call.gas_limit) } else { None },
            ..Default::default()
        };

        if let Some(auth_list) = active_delegation.take() {
            tx_req.authorization_list = Some(vec![auth_list]);
        } else {
            tx_req.authorization_list = None;
        }

        broadcastable_transactions.push_back(BroadcastableTransaction {
            rpc: ecx_inner.db.active_fork_url(),
            transaction: tx_req.into(),
        });
        debug!(target: "cheatcodes", tx=?broadcastable_transactions.back().unwrap(), "broadcastable call");
    }
}

impl CheatcodeInspectorStrategyExt for EvmCheatcodeInspectorStrategy {
    fn new_cloned_ext(&self) -> Arc<Mutex<dyn CheatcodeInspectorStrategyExt>> {
        Arc::new(Mutex::new(self.clone()))
    }
}

struct _ObjectSafe0(dyn CheatcodeInspectorStrategy);
struct _ObjectSafe1(dyn CheatcodeInspectorStrategyExt);
