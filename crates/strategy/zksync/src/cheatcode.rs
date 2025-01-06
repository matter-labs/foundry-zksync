use std::{any::TypeId, fs, path::PathBuf, sync::Arc};

use alloy_json_abi::ContractObject;
use alloy_primitives::{keccak256, map::HashMap, Address, Bytes, TxKind, B256, U256};
use alloy_rpc_types::{
    request::{TransactionInput, TransactionRequest},
    serde_helpers::WithOtherFields,
};
use alloy_sol_types::SolValue;
use foundry_cheatcodes::{
    journaled_account, make_acc_non_empty,
    strategy::{
        CheatcodeInspectorStrategy, CheatcodeInspectorStrategyContext,
        CheatcodeInspectorStrategyExt, CheatcodeInspectorStrategyRunner,
        EvmCheatcodeInspectorStrategyRunner,
    },
    Broadcast, BroadcastableTransaction, BroadcastableTransactions, Cheatcodes, CheatcodesExecutor,
    CheatsConfig, CheatsCtxt, CommonCreateInput, DealRecord, DynCheatcode, Ecx, Error, InnerEcx,
    Result,
    Vm::{
        self, dealCall, etchCall, getCodeCall, getNonce_0Call, mockCallRevert_0Call,
        mockCall_0Call, resetNonceCall, rollCall, setNonceCall, setNonceUnsafeCall, warpCall,
        zkRegisterContractCall, zkUseFactoryDepCall, zkUsePaymasterCall, zkVmCall, zkVmSkipCall,
    },
};
use foundry_common::TransactionMaybeSigned;
use foundry_config::fs_permissions::FsAccessKind;
use foundry_evm::{
    backend::{DatabaseError, LocalForkId},
    constants::{DEFAULT_CREATE2_DEPLOYER, DEFAULT_CREATE2_DEPLOYER_CODE},
};
use foundry_evm_core::{
    backend::DatabaseExt,
    constants::{CHEATCODE_ADDRESS, CHEATCODE_CONTRACT_HASH},
};
use foundry_zksync_compilers::dual_compiled_contracts::{
    ContractType, DualCompiledContract, DualCompiledContracts,
};
use foundry_zksync_core::{
    convert::{ConvertAddress, ConvertH160, ConvertH256, ConvertRU256, ConvertU256},
    get_account_code_key, get_balance_key, get_nonce_key,
    vm::ZkEnv,
    PaymasterParams, ZkPaymasterData, ZkTransactionMetadata, ACCOUNT_CODE_STORAGE_ADDRESS,
    CONTRACT_DEPLOYER_ADDRESS, DEFAULT_CREATE2_DEPLOYER_ZKSYNC, H256, KNOWN_CODES_STORAGE_ADDRESS,
    L2_BASE_TOKEN_ADDRESS, NONCE_HOLDER_ADDRESS, ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY,
};
use itertools::Itertools;
use revm::{
    interpreter::{
        opcode as op, CallInputs, CallOutcome, CreateOutcome, Gas, InstructionResult, Interpreter,
        InterpreterResult,
    },
    primitives::{
        AccountInfo, Bytecode, CreateScheme, Env, EvmStorageSlot, ExecutionResult, HashSet, Output,
        SignedAuthorization, KECCAK_EMPTY,
    },
};
use semver::Version;
use tracing::{debug, error, info, warn};
use zksync_types::{
    block::{pack_block_info, unpack_block_info},
    utils::{decompose_full_nonce, nonces_to_full_nonce},
    CURRENT_VIRTUAL_BLOCK_INFO_POSITION, SYSTEM_CONTEXT_ADDRESS,
};

macro_rules! fmt_err {
    ($msg:literal $(,)?) => {
        Error::fmt(::std::format_args!($msg))
    };
    ($err:expr $(,)?) => {
        <Error as ::std::convert::From<_>>::from($err)
    };
    ($fmt:expr, $($arg:tt)*) => {
        Error::fmt(::std::format_args!($fmt, $($arg)*))
    };
}

macro_rules! bail {
    ($msg:literal $(,)?) => {
        return ::std::result::Result::Err(fmt_err!($msg))
    };
    ($err:expr $(,)?) => {
        return ::std::result::Result::Err(fmt_err!($err))
    };
    ($fmt:expr, $($arg:tt)*) => {
        return ::std::result::Result::Err(fmt_err!($fmt, $($arg)*))
    };
}

/// ZKsync implementation for [CheatcodeInspectorStrategyRunner].
#[derive(Debug, Default, Clone)]
pub struct ZksyncCheatcodeInspectorStrategyRunner {
    evm: EvmCheatcodeInspectorStrategyRunner,
}

/// Context for [ZksyncCheatcodeInspectorStrategyRunner].
#[derive(Debug, Default, Clone)]
pub struct ZksyncCheatcodeInspectorStrategyContext {
    pub using_zk_vm: bool,

    /// When in zkEVM context, execute the next CALL or CREATE in the EVM instead.
    pub skip_zk_vm: bool,

    /// Any contracts that were deployed in `skip_zk_vm` step.
    /// This makes it easier to dispatch calls to any of these addresses in zkEVM context, directly
    /// to EVM. Alternatively, we'd need to add `vm.zkVmSkip()` to these calls manually.
    pub skip_zk_vm_addresses: HashSet<Address>,

    /// Records the next create address for `skip_zk_vm_addresses`.
    pub record_next_create_address: bool,

    /// Paymaster params
    pub paymaster_params: Option<ZkPaymasterData>,

    /// Dual compiled contracts
    pub dual_compiled_contracts: DualCompiledContracts,

    /// The migration status of the database to zkEVM storage, `None` if we start in EVM context.
    pub zk_startup_migration: ZkStartupMigration,

    /// Factory deps stored through `zkUseFactoryDep`. These factory deps are used in the next
    /// CREATE or CALL, and cleared after.
    pub zk_use_factory_deps: Vec<String>,

    /// The list of factory_deps seen so far during a test or script execution.
    /// Ideally these would be persisted in the storage, but since modifying [revm::JournaledState]
    /// would be a significant refactor, we maintain the factory_dep part in the [Cheatcodes].
    /// This can be done as each test runs with its own [Cheatcodes] instance, thereby
    /// providing the necessary level of isolation.
    pub persisted_factory_deps: HashMap<H256, Vec<u8>>,

    /// Nonce update persistence behavior in zkEVM for the tx caller.
    pub zk_persist_nonce_update: ZkPersistNonceUpdate,

    /// Stores the factory deps that were detected as part of CREATE2 deployer call.
    /// Must be cleared every call.
    pub set_deployer_call_input_factory_deps: Vec<Vec<u8>>,

    /// Era Vm environment
    pub zk_env: ZkEnv,
}

impl ZksyncCheatcodeInspectorStrategyContext {
    pub fn new(dual_compiled_contracts: DualCompiledContracts, zk_env: ZkEnv) -> Self {
        // We add the empty bytecode manually so it is correctly translated in zk mode.
        // This is used in many places in foundry, e.g. in cheatcode contract's account code.
        let empty_bytes = Bytes::from_static(&[0]);
        let zk_bytecode_hash = foundry_zksync_core::hash_bytecode(&foundry_zksync_core::EMPTY_CODE);
        let zk_deployed_bytecode = foundry_zksync_core::EMPTY_CODE.to_vec();

        let mut dual_compiled_contracts = dual_compiled_contracts;
        dual_compiled_contracts.push(DualCompiledContract {
            name: String::from("EmptyEVMBytecode"),
            zk_bytecode_hash,
            zk_deployed_bytecode: zk_deployed_bytecode.clone(),
            zk_factory_deps: Default::default(),
            evm_bytecode_hash: B256::from_slice(&keccak256(&empty_bytes)[..]),
            evm_deployed_bytecode: Bytecode::new_raw(empty_bytes.clone()).bytecode().to_vec(),
            evm_bytecode: Bytecode::new_raw(empty_bytes).bytecode().to_vec(),
        });

        let cheatcodes_bytecode = {
            let mut bytecode = CHEATCODE_ADDRESS.abi_encode_packed();
            bytecode.append(&mut [0; 12].to_vec());
            Bytes::from(bytecode)
        };
        dual_compiled_contracts.push(DualCompiledContract {
            name: String::from("CheatcodeBytecode"),
            // we put a different bytecode hash here so when importing back to EVM
            // we avoid collision with EmptyEVMBytecode for the cheatcodes
            zk_bytecode_hash: foundry_zksync_core::hash_bytecode(CHEATCODE_CONTRACT_HASH.as_ref()),
            zk_deployed_bytecode: cheatcodes_bytecode.to_vec(),
            zk_factory_deps: Default::default(),
            evm_bytecode_hash: CHEATCODE_CONTRACT_HASH,
            evm_deployed_bytecode: cheatcodes_bytecode.to_vec(),
            evm_bytecode: cheatcodes_bytecode.to_vec(),
        });

        let mut persisted_factory_deps = HashMap::new();
        persisted_factory_deps.insert(zk_bytecode_hash, zk_deployed_bytecode);

        Self {
            using_zk_vm: false, // We need to migrate once on initialize_interp
            skip_zk_vm: false,
            skip_zk_vm_addresses: Default::default(),
            record_next_create_address: Default::default(),
            paymaster_params: Default::default(),
            dual_compiled_contracts,
            zk_startup_migration: ZkStartupMigration::Defer,
            zk_use_factory_deps: Default::default(),
            persisted_factory_deps: Default::default(),
            zk_persist_nonce_update: Default::default(),
            set_deployer_call_input_factory_deps: Default::default(),
            zk_env,
        }
    }
}

impl CheatcodeInspectorStrategyContext for ZksyncCheatcodeInspectorStrategyContext {
    fn new_cloned(&self) -> Box<dyn CheatcodeInspectorStrategyContext> {
        Box::new(self.clone())
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn as_any_ref(&self) -> &dyn std::any::Any {
        self
    }
}

/// Allows overriding nonce update behavior for the tx caller in the zkEVM.
///
/// Since each CREATE or CALL is executed as a separate transaction within zkEVM, we currently skip
/// persisting nonce updates as it erroneously increments the tx nonce. However, under certain
/// situations, e.g. deploying contracts, transacts, etc. the nonce updates must be persisted.
#[derive(Default, Debug, Clone)]
pub enum ZkPersistNonceUpdate {
    /// Never update the nonce. This is currently the default behavior.
    #[default]
    Never,
    /// Override the default behavior, and persist nonce update for tx caller for the next
    /// zkEVM execution _only_.
    PersistNext,
}

impl ZkPersistNonceUpdate {
    /// Persist nonce update for the tx caller for next execution.
    pub fn persist_next(&mut self) {
        *self = Self::PersistNext;
    }

    /// Retrieve if a nonce update must be persisted, or not. Resets the state to default.
    pub fn check(&mut self) -> bool {
        let persist_nonce_update = match self {
            Self::Never => false,
            Self::PersistNext => true,
        };
        *self = Default::default();

        persist_nonce_update
    }
}

impl CheatcodeInspectorStrategyRunner for ZksyncCheatcodeInspectorStrategyRunner {
    fn name(&self) -> &'static str {
        "zk"
    }

    fn new_cloned(&self) -> Box<dyn CheatcodeInspectorStrategyRunner> {
        Box::new(self.clone())
    }

    fn base_contract_deployed(&self, ctx: &mut dyn CheatcodeInspectorStrategyContext) {
        let ctx = get_context(ctx);

        debug!("allowing startup storage migration");
        ctx.zk_startup_migration.allow();
        debug!("allowing persisting next nonce update");
        ctx.zk_persist_nonce_update.persist_next();
    }

    fn apply_full(
        &self,
        cheatcode: &dyn DynCheatcode,
        ccx: &mut CheatsCtxt<'_, '_, '_, '_>,
        executor: &mut dyn CheatcodesExecutor,
    ) -> Result {
        fn is<T: std::any::Any>(t: TypeId) -> bool {
            TypeId::of::<T>() == t
        }

        let ctx = get_context(ccx.state.strategy.context.as_mut());

        // Try to downcast the cheatcode to a type that requires special handling.
        // Note that some cheatcodes are only handled in zkEVM context.
        // If no handler fires, we use the default execution logic.
        match cheatcode.as_any().type_id() {
            t if ctx.using_zk_vm && is::<etchCall>(t) => {
                let etchCall { target, newRuntimeBytecode } =
                    cheatcode.as_any().downcast_ref().unwrap();
                foundry_zksync_core::cheatcodes::etch(*target, newRuntimeBytecode, ccx.ecx);
                Ok(Default::default())
            }
            t if ctx.using_zk_vm && is::<rollCall>(t) => {
                let &rollCall { newHeight } = cheatcode.as_any().downcast_ref().unwrap();
                ccx.ecx.env.block.number = newHeight;
                foundry_zksync_core::cheatcodes::roll(newHeight, ccx.ecx);
                Ok(Default::default())
            }
            t if ctx.using_zk_vm && is::<warpCall>(t) => {
                let &warpCall { newTimestamp } = cheatcode.as_any().downcast_ref().unwrap();
                ccx.ecx.env.block.number = newTimestamp;
                foundry_zksync_core::cheatcodes::warp(newTimestamp, ccx.ecx);
                Ok(Default::default())
            }
            t if ctx.using_zk_vm && is::<dealCall>(t) => {
                let &dealCall { account, newBalance } = cheatcode.as_any().downcast_ref().unwrap();

                let old_balance =
                    foundry_zksync_core::cheatcodes::deal(account, newBalance, ccx.ecx);
                let record = DealRecord { address: account, old_balance, new_balance: newBalance };
                ccx.state.eth_deals.push(record);
                Ok(Default::default())
            }
            t if ctx.using_zk_vm && is::<resetNonceCall>(t) => {
                let &resetNonceCall { account } = cheatcode.as_any().downcast_ref().unwrap();
                foundry_zksync_core::cheatcodes::set_nonce(account, U256::ZERO, ccx.ecx);
                Ok(Default::default())
            }
            t if ctx.using_zk_vm && is::<setNonceCall>(t) => {
                let &setNonceCall { account, newNonce } =
                    cheatcode.as_any().downcast_ref().unwrap();

                // nonce must increment only
                let current = foundry_zksync_core::cheatcodes::get_nonce(account, ccx.ecx);
                if U256::from(newNonce) < current {
                    return Err(fmt_err!(
                        "new nonce ({newNonce}) must be strictly equal to or higher than the \
                    account's current nonce ({current})"
                    ));
                }

                foundry_zksync_core::cheatcodes::set_nonce(account, U256::from(newNonce), ccx.ecx);
                Ok(Default::default())
            }
            t if ctx.using_zk_vm && is::<setNonceUnsafeCall>(t) => {
                let &setNonceUnsafeCall { account, newNonce } =
                    cheatcode.as_any().downcast_ref().unwrap();
                foundry_zksync_core::cheatcodes::set_nonce(account, U256::from(newNonce), ccx.ecx);
                Ok(Default::default())
            }
            t if ctx.using_zk_vm && is::<getNonce_0Call>(t) => {
                let &getNonce_0Call { account } = cheatcode.as_any().downcast_ref().unwrap();

                let nonce = foundry_zksync_core::cheatcodes::get_nonce(account, ccx.ecx);
                Ok(nonce.abi_encode())
            }
            t if ctx.using_zk_vm && is::<mockCall_0Call>(t) => {
                let mockCall_0Call { callee, data, returnData } =
                    cheatcode.as_any().downcast_ref().unwrap();

                let _ = foundry_cheatcodes::make_acc_non_empty(callee, ccx.ecx)?;
                foundry_zksync_core::cheatcodes::set_mocked_account(*callee, ccx.ecx, ccx.caller);
                foundry_cheatcodes::mock_call(
                    ccx.state,
                    callee,
                    data,
                    None,
                    returnData,
                    InstructionResult::Return,
                );
                Ok(Default::default())
            }
            t if ctx.using_zk_vm && is::<mockCallRevert_0Call>(t) => {
                let mockCallRevert_0Call { callee, data, revertData } =
                    cheatcode.as_any().downcast_ref().unwrap();

                let _ = make_acc_non_empty(callee, ccx.ecx)?;
                foundry_zksync_core::cheatcodes::set_mocked_account(*callee, ccx.ecx, ccx.caller);
                // not calling
                foundry_cheatcodes::mock_call(
                    ccx.state,
                    callee,
                    data,
                    None,
                    revertData,
                    InstructionResult::Revert,
                );
                Ok(Default::default())
            }
            t if is::<getCodeCall>(t) => {
                // We don't need to check for `using_zk_vm` since we pass it to `get_artifact_code`.
                let getCodeCall { artifactPath } = cheatcode.as_any().downcast_ref().unwrap();

                Ok(get_artifact_code(
                    &ctx.dual_compiled_contracts,
                    ctx.using_zk_vm,
                    &ccx.state.config,
                    artifactPath,
                    false,
                )?
                .abi_encode())
            }
            t if is::<zkVmCall>(t) => {
                let zkVmCall { enable } = cheatcode.as_any().downcast_ref().unwrap();
                if *enable {
                    self.select_zk_vm(ctx, ccx.ecx, None)
                } else {
                    self.select_evm(ctx, ccx.ecx);
                }
                Ok(Default::default())
            }
            t if is::<zkVmSkipCall>(t) => {
                let zkVmSkipCall { .. } = cheatcode.as_any().downcast_ref().unwrap();
                let ctx = get_context(ctx);
                ctx.skip_zk_vm = true;
                Ok(Default::default())
            }
            t if is::<zkUsePaymasterCall>(t) => {
                let zkUsePaymasterCall { paymaster_address, paymaster_input } =
                    cheatcode.as_any().downcast_ref().unwrap();
                ctx.paymaster_params = Some(ZkPaymasterData {
                    address: *paymaster_address,
                    input: paymaster_input.clone(),
                });
                Ok(Default::default())
            }
            t if is::<zkUseFactoryDepCall>(t) => {
                let zkUseFactoryDepCall { name } = cheatcode.as_any().downcast_ref().unwrap();
                info!("Adding factory dependency: {:?}", name);
                ctx.zk_use_factory_deps.push(name.clone());
                Ok(Default::default())
            }
            t if is::<zkRegisterContractCall>(t) => {
                let zkRegisterContractCall {
                    name,
                    evmBytecodeHash,
                    evmDeployedBytecode,
                    evmBytecode,
                    zkBytecodeHash,
                    zkDeployedBytecode,
                } = cheatcode.as_any().downcast_ref().unwrap();

                let zk_factory_deps = vec![]; //TODO: add argument to cheatcode
                let new_contract = DualCompiledContract {
                    name: name.clone(),
                    zk_bytecode_hash: H256(zkBytecodeHash.0),
                    zk_deployed_bytecode: zkDeployedBytecode.to_vec(),
                    zk_factory_deps,
                    evm_bytecode_hash: *evmBytecodeHash,
                    evm_deployed_bytecode: evmDeployedBytecode.to_vec(),
                    evm_bytecode: evmBytecode.to_vec(),
                };

                if let Some(existing) = ctx.dual_compiled_contracts.iter().find(|contract| {
                    contract.evm_bytecode_hash == new_contract.evm_bytecode_hash &&
                        contract.zk_bytecode_hash == new_contract.zk_bytecode_hash
                }) {
                    warn!(
                        name = existing.name,
                        "contract already exists with the given bytecode hashes"
                    );
                    return Ok(Default::default())
                }

                ctx.dual_compiled_contracts.push(new_contract);

                Ok(Default::default())
            }
            _ => {
                // Not custom, just invoke the default behavior
                cheatcode.dyn_apply(ccx, executor)
            }
        }
    }

    fn record_broadcastable_create_transactions(
        &self,
        ctx: &mut dyn CheatcodeInspectorStrategyContext,
        config: Arc<CheatsConfig>,
        input: &dyn CommonCreateInput,
        ecx_inner: InnerEcx<'_, '_, '_>,
        broadcast: &Broadcast,
        broadcastable_transactions: &mut BroadcastableTransactions,
    ) {
        let ctx_zk = get_context(ctx);
        if !ctx_zk.using_zk_vm {
            return self.evm.record_broadcastable_create_transactions(
                ctx,
                config,
                input,
                ecx_inner,
                broadcast,
                broadcastable_transactions,
            );
        }

        let ctx = ctx_zk;

        let is_fixed_gas_limit =
            foundry_cheatcodes::check_if_fixed_gas_limit(ecx_inner, input.gas_limit());

        let init_code = input.init_code();
        let to = Some(TxKind::Call(CONTRACT_DEPLOYER_ADDRESS.to_address()));
        let mut nonce = foundry_zksync_core::nonce(broadcast.new_origin, ecx_inner) as u64;
        let find_contract = ctx
            .dual_compiled_contracts
            .find_bytecode(&init_code.0)
            .unwrap_or_else(|| panic!("failed finding contract for {init_code:?}"));

        let constructor_args = find_contract.constructor_args();
        let contract = find_contract.contract();

        let factory_deps = ctx.dual_compiled_contracts.fetch_all_factory_deps(contract);

        let create_input = foundry_zksync_core::encode_create_params(
            &input.scheme().unwrap_or(CreateScheme::Create),
            contract.zk_bytecode_hash,
            constructor_args.to_vec(),
        );
        let call_init_code = Bytes::from(create_input);

        let mut zk_tx_factory_deps = factory_deps;

        let paymaster_params = ctx.paymaster_params.clone().map(|paymaster_data| PaymasterParams {
            paymaster: paymaster_data.address.to_h160(),
            paymaster_input: paymaster_data.input.to_vec(),
        });

        let rpc = ecx_inner.db.active_fork_url();

        let injected_factory_deps = ctx
            .zk_use_factory_deps
            .iter()
            .map(|contract| {
                get_artifact_code(
                    &ctx.dual_compiled_contracts,
                    ctx.using_zk_vm,
                    &config,
                    contract,
                    false,
                )
                .inspect(|_| info!(contract, "pushing factory dep"))
                .unwrap_or_else(|_| {
                    panic!("failed to get bytecode for factory deps contract {contract}")
                })
                .to_vec()
            })
            .collect_vec();
        zk_tx_factory_deps.extend(injected_factory_deps);
        let mut batched = foundry_zksync_core::vm::batch_factory_dependencies(zk_tx_factory_deps);
        debug!(batches = batched.len(), "splitting factory deps for broadcast");
        // the last batch is the final one that does the deployment
        zk_tx_factory_deps = batched.pop().expect("must have at least 1 item");

        for factory_deps in batched {
            let mut tx = WithOtherFields::new(TransactionRequest {
                from: Some(broadcast.new_origin),
                to: Some(TxKind::Call(Address::ZERO)),
                value: Some(input.value()),
                nonce: Some(nonce),
                ..Default::default()
            });
            tx.other.insert(
                ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY.to_string(),
                serde_json::to_value(ZkTransactionMetadata::new(
                    factory_deps,
                    paymaster_params.clone(),
                ))
                .expect("failed encoding json"),
            );

            broadcastable_transactions.push_back(BroadcastableTransaction {
                rpc: rpc.clone(),
                transaction: TransactionMaybeSigned::Unsigned(tx),
            });

            //update nonce for each tx
            nonce += 1;
        }

        let mut tx = WithOtherFields::new(TransactionRequest {
            from: Some(broadcast.new_origin),
            to,
            value: Some(input.value()),
            input: TransactionInput::new(call_init_code),
            nonce: Some(nonce),
            gas: if is_fixed_gas_limit { Some(input.gas_limit()) } else { None },
            ..Default::default()
        });
        tx.other.insert(
            ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY.to_string(),
            serde_json::to_value(ZkTransactionMetadata::new(zk_tx_factory_deps, paymaster_params))
                .expect("failed encoding json"),
        );
        broadcastable_transactions.push_back(BroadcastableTransaction {
            rpc,
            transaction: TransactionMaybeSigned::Unsigned(tx),
        });
    }

    fn record_broadcastable_call_transactions(
        &self,
        ctx: &mut dyn CheatcodeInspectorStrategyContext,
        config: Arc<CheatsConfig>,
        call: &CallInputs,
        ecx_inner: InnerEcx<'_, '_, '_>,
        broadcast: &Broadcast,
        broadcastable_transactions: &mut BroadcastableTransactions,
        active_delegation: &mut Option<SignedAuthorization>,
    ) {
        let ctx_zk = get_context(ctx);

        if !ctx_zk.using_zk_vm {
            return self.evm.record_broadcastable_call_transactions(
                ctx,
                config,
                call,
                ecx_inner,
                broadcast,
                broadcastable_transactions,
                active_delegation,
            );
        }

        let ctx = ctx_zk;

        let is_fixed_gas_limit =
            foundry_cheatcodes::check_if_fixed_gas_limit(ecx_inner, call.gas_limit);

        let nonce = foundry_zksync_core::nonce(broadcast.new_origin, ecx_inner) as u64;

        let factory_deps = &mut ctx.set_deployer_call_input_factory_deps;
        let injected_factory_deps = ctx
            .zk_use_factory_deps
            .iter()
            .flat_map(|contract| {
                let artifact_code = get_artifact_code(
                    &ctx.dual_compiled_contracts,
                    ctx.using_zk_vm,
                    &config,
                    contract,
                    false,
                )
                .inspect(|_| info!(contract, "pushing factory dep"))
                .unwrap_or_else(|_| {
                    panic!("failed to get bytecode for factory deps contract {contract}")
                })
                .to_vec();
                let res = ctx.dual_compiled_contracts.find_bytecode(&artifact_code).unwrap();
                ctx.dual_compiled_contracts.fetch_all_factory_deps(res.contract())
            })
            .collect_vec();
        factory_deps.extend(injected_factory_deps.clone());

        let paymaster_params = ctx.paymaster_params.clone().map(|paymaster_data| PaymasterParams {
            paymaster: paymaster_data.address.to_h160(),
            paymaster_input: paymaster_data.input.to_vec(),
        });
        let factory_deps = if call.target_address == DEFAULT_CREATE2_DEPLOYER_ZKSYNC {
            // We shouldn't need factory_deps for CALLs
            factory_deps.clone()
        } else {
            // For this case we use only the injected factory deps
            injected_factory_deps
        };
        let zk_tx = ZkTransactionMetadata::new(factory_deps, paymaster_params);

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
        let mut tx = WithOtherFields::new(tx_req);

        tx.other.insert(
            ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY.to_string(),
            serde_json::to_value(zk_tx).expect("failed encoding json"),
        );

        broadcastable_transactions.push_back(BroadcastableTransaction {
            rpc: ecx_inner.db.active_fork_url(),
            transaction: TransactionMaybeSigned::Unsigned(tx),
        });
        debug!(target: "cheatcodes", tx=?broadcastable_transactions.back().unwrap(), "broadcastable call");
    }

    fn post_initialize_interp(
        &self,
        ctx: &mut dyn CheatcodeInspectorStrategyContext,
        _interpreter: &mut Interpreter,
        ecx: Ecx<'_, '_, '_>,
    ) {
        let ctx = get_context(ctx);

        if ctx.zk_startup_migration.is_allowed() && !ctx.using_zk_vm {
            self.select_zk_vm(ctx, ecx, None);
            ctx.zk_startup_migration.done();
            debug!("startup zkEVM storage migration completed");
        }
    }

    /// Returns true if handled.
    fn pre_step_end(
        &self,
        ctx: &mut dyn CheatcodeInspectorStrategyContext,
        interpreter: &mut Interpreter,
        ecx: Ecx<'_, '_, '_>,
    ) -> bool {
        // override address(x).balance retrieval to make it consistent between EraVM and EVM
        let ctx = get_context(ctx);

        if !ctx.using_zk_vm {
            return false;
        }

        let address = match interpreter.current_opcode() {
            op::SELFBALANCE => interpreter.contract().target_address,
            op::BALANCE => {
                if interpreter.stack.is_empty() {
                    interpreter.instruction_result = InstructionResult::StackUnderflow;
                    return true;
                }

                Address::from_word(B256::from(unsafe { interpreter.stack.pop_unsafe() }))
            }
            _ => return true,
        };

        // Safety: Length is checked above.
        let balance = foundry_zksync_core::balance(address, ecx);

        // Skip the current BALANCE instruction since we've already handled it
        match interpreter.stack.push(balance) {
            Ok(_) => unsafe {
                interpreter.instruction_pointer = interpreter.instruction_pointer.add(1);
            },
            Err(e) => {
                interpreter.instruction_result = e;
            }
        };

        false
    }
}

impl CheatcodeInspectorStrategyExt for ZksyncCheatcodeInspectorStrategyRunner {
    fn zksync_record_create_address(
        &self,
        ctx: &mut dyn CheatcodeInspectorStrategyContext,
        outcome: &CreateOutcome,
    ) {
        let ctx = get_context(ctx);

        if ctx.record_next_create_address {
            ctx.record_next_create_address = false;
            if let Some(address) = outcome.address {
                ctx.skip_zk_vm_addresses.insert(address);
            }
        }
    }

    fn zksync_sync_nonce(
        &self,
        _ctx: &mut dyn CheatcodeInspectorStrategyContext,
        sender: Address,
        nonce: u64,
        ecx: Ecx<'_, '_, '_>,
    ) {
        // NOTE(zk): We sync with the nonce changes to ensure that the nonce matches
        foundry_zksync_core::cheatcodes::set_nonce(sender, U256::from(nonce), ecx);
    }

    fn zksync_set_deployer_call_input(
        &self,
        ctx: &mut dyn CheatcodeInspectorStrategyContext,
        call: &mut CallInputs,
    ) {
        let ctx = get_context(ctx);

        ctx.set_deployer_call_input_factory_deps.clear();
        if call.target_address == DEFAULT_CREATE2_DEPLOYER && ctx.using_zk_vm {
            call.target_address = DEFAULT_CREATE2_DEPLOYER_ZKSYNC;
            call.bytecode_address = DEFAULT_CREATE2_DEPLOYER_ZKSYNC;

            let (salt, init_code) = call.input.split_at(32);
            let find_contract = ctx
                .dual_compiled_contracts
                .find_bytecode(init_code)
                .unwrap_or_else(|| panic!("failed finding contract for {init_code:?}"));

            let constructor_args = find_contract.constructor_args();
            let contract = find_contract.contract();

            // store these for broadcast reasons
            ctx.set_deployer_call_input_factory_deps =
                ctx.dual_compiled_contracts.fetch_all_factory_deps(contract);

            let create_input = foundry_zksync_core::encode_create_params(
                &CreateScheme::Create2 { salt: U256::from_be_slice(salt) },
                contract.zk_bytecode_hash,
                constructor_args.to_vec(),
            );

            call.input = create_input.into();
        }
    }

    /// Try handling the `CREATE` within zkEVM.
    /// If `Some` is returned then the result must be returned immediately, else the call must be
    /// handled in EVM.
    fn zksync_try_create(
        &self,
        state: &mut Cheatcodes,
        ecx: Ecx<'_, '_, '_>,
        input: &dyn CommonCreateInput,
        executor: &mut dyn CheatcodesExecutor,
    ) -> Option<CreateOutcome> {
        let ctx = get_context(state.strategy.context.as_mut());

        if !ctx.using_zk_vm {
            return None;
        }

        if ctx.skip_zk_vm {
            ctx.skip_zk_vm = false; // handled the skip, reset flag
            ctx.record_next_create_address = true;
            info!("running create in EVM, instead of zkEVM (skipped)");
            return None
        }

        if let Some(CreateScheme::Create) = input.scheme() {
            let caller = input.caller();
            let nonce = ecx
                .inner
                .journaled_state
                .load_account(input.caller(), &mut ecx.inner.db)
                .expect("to load caller account")
                .info
                .nonce;
            let address = caller.create(nonce);
            if ecx.db.get_test_contract_address().map(|addr| address == addr).unwrap_or_default() {
                info!("running create in EVM, instead of zkEVM (Test Contract) {:#?}", address);
                return None
            }
        }

        let init_code = input.init_code();
        if init_code.0 == DEFAULT_CREATE2_DEPLOYER_CODE {
            info!("running create in EVM, instead of zkEVM (DEFAULT_CREATE2_DEPLOYER_CODE)");
            return None
        }

        info!("running create in zkEVM");

        let find_contract = ctx
            .dual_compiled_contracts
            .find_bytecode(&init_code.0)
            .unwrap_or_else(|| panic!("failed finding contract for {init_code:?}"));

        let constructor_args = find_contract.constructor_args();
        let contract = find_contract.contract();

        let zk_create_input = foundry_zksync_core::encode_create_params(
            &input.scheme().unwrap_or(CreateScheme::Create),
            contract.zk_bytecode_hash,
            constructor_args.to_vec(),
        );

        let mut factory_deps = ctx.dual_compiled_contracts.fetch_all_factory_deps(contract);
        let injected_factory_deps = ctx
            .zk_use_factory_deps
            .iter()
            .flat_map(|contract| {
                let artifact_code = get_artifact_code(
                    &ctx.dual_compiled_contracts,
                    ctx.using_zk_vm,
                    &state.config,
                    contract,
                    false,
                )
                .inspect(|_| info!(contract, "pushing factory dep"))
                .unwrap_or_else(|_| {
                    panic!("failed to get bytecode for injected factory deps contract {contract}")
                })
                .to_vec();
                let res = ctx.dual_compiled_contracts.find_bytecode(&artifact_code).unwrap();
                ctx.dual_compiled_contracts.fetch_all_factory_deps(res.contract())
            })
            .collect_vec();
        factory_deps.extend(injected_factory_deps);

        // NOTE(zk): Clear injected factory deps so that they are not sent on further transactions
        ctx.zk_use_factory_deps.clear();
        tracing::debug!(contract = contract.name, "using dual compiled contract");

        let zk_persist_nonce_update = ctx.zk_persist_nonce_update.check();
        let ccx = foundry_zksync_core::vm::CheatcodeTracerContext {
            mocked_calls: state.mocked_calls.clone(),
            expected_calls: Some(&mut state.expected_calls),
            accesses: state.accesses.as_mut(),
            persisted_factory_deps: Some(&mut ctx.persisted_factory_deps),
            paymaster_data: ctx.paymaster_params.take(),
            persist_nonce_update: state.broadcast.is_some() || zk_persist_nonce_update,
            zk_env: ctx.zk_env.clone(),
        };

        let zk_create = foundry_zksync_core::vm::ZkCreateInputs {
            value: input.value().to_u256(),
            msg_sender: input.caller(),
            create_input: zk_create_input,
            factory_deps,
        };

        let mut gas = Gas::new(input.gas_limit());
        match foundry_zksync_core::vm::create::<_, DatabaseError>(zk_create, ecx, ccx) {
            Ok(result) => {
                if let Some(recorded_logs) = &mut state.recorded_logs {
                    recorded_logs.extend(result.logs.clone().into_iter().map(|log| Vm::Log {
                        topics: log.data.topics().to_vec(),
                        data: log.data.data.clone(),
                        emitter: log.address,
                    }));
                }

                // append console logs from zkEVM to the current executor's LogTracer
                result.logs.iter().filter_map(foundry_evm::decode::decode_console_log).for_each(
                    |decoded_log| {
                        executor.console_log(
                            &mut CheatsCtxt {
                                state,
                                ecx: &mut ecx.inner,
                                precompiles: &mut ecx.precompiles,
                                gas_limit: input.gas_limit(),
                                caller: input.caller(),
                            },
                            decoded_log,
                        );
                    },
                );

                // append traces
                executor.trace_zksync(state, ecx, result.call_traces);

                // for each log in cloned logs call handle_expect_emit
                if !state.expected_emits.is_empty() {
                    for log in result.logs {
                        foundry_cheatcodes::handle_expect_emit(
                            state,
                            &log,
                            &mut Default::default(),
                        );
                    }
                }

                // record immutable variables
                if result.execution_result.is_success() {
                    for (addr, imm_values) in result.recorded_immutables {
                        let addr = addr.to_address();
                        let keys = imm_values
                            .into_keys()
                            .map(|slot_index| {
                                foundry_zksync_core::get_immutable_slot_key(addr, slot_index)
                                    .to_ru256()
                            })
                            .collect::<HashSet<_>>();
                        let strategy = ecx.db.get_strategy();
                        strategy.runner.zksync_save_immutable_storage(
                            strategy.context.as_mut(),
                            addr,
                            keys,
                        );
                    }
                }

                match result.execution_result {
                    ExecutionResult::Success { output, gas_used, .. } => {
                        let _ = gas.record_cost(gas_used);
                        match output {
                            Output::Create(bytes, address) => Some(CreateOutcome {
                                result: InterpreterResult {
                                    result: InstructionResult::Return,
                                    output: bytes,
                                    gas,
                                },
                                address,
                            }),
                            _ => Some(CreateOutcome {
                                result: InterpreterResult {
                                    result: InstructionResult::Revert,
                                    output: Bytes::new(),
                                    gas,
                                },
                                address: None,
                            }),
                        }
                    }
                    ExecutionResult::Revert { output, gas_used, .. } => {
                        let _ = gas.record_cost(gas_used);
                        Some(CreateOutcome {
                            result: InterpreterResult {
                                result: InstructionResult::Revert,
                                output,
                                gas,
                            },
                            address: None,
                        })
                    }
                    ExecutionResult::Halt { .. } => Some(CreateOutcome {
                        result: InterpreterResult {
                            result: InstructionResult::Revert,
                            output: Bytes::from_iter(String::from("zk vm halted").as_bytes()),
                            gas,
                        },
                        address: None,
                    }),
                }
            }
            Err(err) => {
                error!("error inspecting zkEVM: {err:?}");
                Some(CreateOutcome {
                    result: InterpreterResult {
                        result: InstructionResult::Revert,
                        output: Bytes::from_iter(
                            format!("error inspecting zkEVM: {err:?}").as_bytes(),
                        ),
                        gas,
                    },
                    address: None,
                })
            }
        }
    }

    /// Try handling the `CALL` within zkEVM.
    /// If `Some` is returned then the result must be returned immediately, else the call must be
    /// handled in EVM.
    fn zksync_try_call(
        &self,
        state: &mut Cheatcodes,
        ecx: Ecx<'_, '_, '_>,
        call: &CallInputs,
        executor: &mut dyn CheatcodesExecutor,
    ) -> Option<CallOutcome> {
        let ctx = get_context(state.strategy.context.as_mut());

        // We need to clear them out for the next call.
        let factory_deps = std::mem::take(&mut ctx.set_deployer_call_input_factory_deps);

        if !ctx.using_zk_vm {
            return None;
        }

        // also skip if the target was created during a zkEVM skip
        ctx.skip_zk_vm = ctx.skip_zk_vm || ctx.skip_zk_vm_addresses.contains(&call.target_address);
        if ctx.skip_zk_vm {
            ctx.skip_zk_vm = false; // handled the skip, reset flag
            info!("running create in EVM, instead of zkEVM (skipped) {:#?}", call);
            return None;
        }

        if ecx
            .db
            .get_test_contract_address()
            .map(|addr| call.bytecode_address == addr)
            .unwrap_or_default()
        {
            info!(
                "running call in EVM, instead of zkEVM (Test Contract) {:#?}",
                call.bytecode_address
            );
            return None
        }

        info!("running call in zkEVM {:#?}", call);
        let zk_persist_nonce_update = ctx.zk_persist_nonce_update.check();

        // NOTE(zk): Clear injected factory deps here even though it's actually used in broadcast.
        // To be consistent with where we clear factory deps in try_create_in_zk.
        ctx.zk_use_factory_deps.clear();

        let ccx = foundry_zksync_core::vm::CheatcodeTracerContext {
            mocked_calls: state.mocked_calls.clone(),
            expected_calls: Some(&mut state.expected_calls),
            accesses: state.accesses.as_mut(),
            persisted_factory_deps: Some(&mut ctx.persisted_factory_deps),
            paymaster_data: ctx.paymaster_params.take(),
            persist_nonce_update: state.broadcast.is_some() || zk_persist_nonce_update,
            zk_env: ctx.zk_env.clone(),
        };

        let mut gas = Gas::new(call.gas_limit);
        match foundry_zksync_core::vm::call::<_, DatabaseError>(call, factory_deps, ecx, ccx) {
            Ok(result) => {
                // append console logs from zkEVM to the current executor's LogTracer
                result.logs.iter().filter_map(foundry_evm::decode::decode_console_log).for_each(
                    |decoded_log| {
                        executor.console_log(
                            &mut CheatsCtxt {
                                state,
                                ecx: &mut ecx.inner,
                                precompiles: &mut ecx.precompiles,
                                gas_limit: call.gas_limit,
                                caller: call.caller,
                            },
                            decoded_log,
                        );
                    },
                );

                // skip log processing for static calls
                if !call.is_static {
                    if let Some(recorded_logs) = &mut state.recorded_logs {
                        recorded_logs.extend(result.logs.clone().into_iter().map(|log| Vm::Log {
                            topics: log.data.topics().to_vec(),
                            data: log.data.data.clone(),
                            emitter: log.address,
                        }));
                    }

                    // append traces
                    executor.trace_zksync(state, ecx, result.call_traces);

                    // for each log in cloned logs call handle_expect_emit
                    if !state.expected_emits.is_empty() {
                        for log in result.logs {
                            foundry_cheatcodes::handle_expect_emit(
                                state,
                                &log,
                                &mut Default::default(),
                            );
                        }
                    }
                }

                match result.execution_result {
                    ExecutionResult::Success { output, gas_used, .. } => {
                        let _ = gas.record_cost(gas_used);
                        match output {
                            Output::Call(bytes) => Some(CallOutcome {
                                result: InterpreterResult {
                                    result: InstructionResult::Return,
                                    output: bytes,
                                    gas,
                                },
                                memory_offset: call.return_memory_offset.clone(),
                            }),
                            _ => Some(CallOutcome {
                                result: InterpreterResult {
                                    result: InstructionResult::Revert,
                                    output: Bytes::new(),
                                    gas,
                                },
                                memory_offset: call.return_memory_offset.clone(),
                            }),
                        }
                    }
                    ExecutionResult::Revert { output, gas_used, .. } => {
                        let _ = gas.record_cost(gas_used);
                        Some(CallOutcome {
                            result: InterpreterResult {
                                result: InstructionResult::Revert,
                                output,
                                gas,
                            },
                            memory_offset: call.return_memory_offset.clone(),
                        })
                    }
                    ExecutionResult::Halt { .. } => Some(CallOutcome {
                        result: InterpreterResult {
                            result: InstructionResult::Revert,
                            output: Bytes::from_iter(String::from("zk vm halted").as_bytes()),
                            gas,
                        },
                        memory_offset: call.return_memory_offset.clone(),
                    }),
                }
            }
            Err(err) => {
                error!("error inspecting zkEVM: {err:?}");
                Some(CallOutcome {
                    result: InterpreterResult {
                        result: InstructionResult::Revert,
                        output: Bytes::from_iter(
                            format!("error inspecting zkEVM: {err:?}").as_bytes(),
                        ),
                        gas,
                    },
                    memory_offset: call.return_memory_offset.clone(),
                })
            }
        }
    }

    fn zksync_select_fork_vm(
        &self,
        ctx: &mut dyn CheatcodeInspectorStrategyContext,
        data: InnerEcx<'_, '_, '_>,
        fork_id: LocalForkId,
    ) {
        let ctx = get_context(ctx);
        self.select_fork_vm(ctx, data, fork_id);
    }
}

impl ZksyncCheatcodeInspectorStrategyRunner {
    /// Selects the appropriate VM for the fork. Options: EVM, ZK-VM.
    /// CALL and CREATE are handled by the selected VM.
    ///
    /// Additionally:
    /// * Translates block information
    /// * Translates all persisted addresses
    pub fn select_fork_vm(
        &self,
        ctx: &mut ZksyncCheatcodeInspectorStrategyContext,
        data: InnerEcx<'_, '_, '_>,
        fork_id: LocalForkId,
    ) {
        let fork_info = data.db.get_fork_info(fork_id).expect("failed getting fork info");
        if fork_info.fork_type.is_evm() {
            self.select_evm(ctx, data)
        } else {
            self.select_zk_vm(ctx, data, Some(&fork_info.fork_env))
        }
    }

    /// Switch to EVM and translate block info, balances, nonces and deployed codes for persistent
    /// accounts
    pub fn select_evm(
        &self,
        ctx: &mut ZksyncCheatcodeInspectorStrategyContext,
        data: InnerEcx<'_, '_, '_>,
    ) {
        if !ctx.using_zk_vm {
            tracing::info!("already in EVM");
            return
        }

        tracing::info!("switching to EVM");
        ctx.using_zk_vm = false;

        let system_account = SYSTEM_CONTEXT_ADDRESS.to_address();
        journaled_account(data, system_account).expect("failed to load account");
        let balance_account = L2_BASE_TOKEN_ADDRESS.to_address();
        journaled_account(data, balance_account).expect("failed to load account");
        let nonce_account = NONCE_HOLDER_ADDRESS.to_address();
        journaled_account(data, nonce_account).expect("failed to load account");
        let account_code_account = ACCOUNT_CODE_STORAGE_ADDRESS.to_address();
        journaled_account(data, account_code_account).expect("failed to load account");

        // TODO we might need to store the deployment nonce under the contract storage
        // to not lose it across VMs.

        let block_info_key = CURRENT_VIRTUAL_BLOCK_INFO_POSITION.to_ru256();
        let block_info = data.sload(system_account, block_info_key).unwrap_or_default();
        let (block_number, block_timestamp) = unpack_block_info(block_info.to_u256());
        data.env.block.number = U256::from(block_number);
        data.env.block.timestamp = U256::from(block_timestamp);

        let test_contract = data.db.get_test_contract_address();
        for address in data.db.persistent_accounts().into_iter().chain([data.env.tx.caller]) {
            info!(?address, "importing to evm state");

            let balance_key = get_balance_key(address);
            let nonce_key = get_nonce_key(address);

            let balance = data.sload(balance_account, balance_key).unwrap_or_default().data;
            let full_nonce = data.sload(nonce_account, nonce_key).unwrap_or_default();
            let (tx_nonce, _deployment_nonce) = decompose_full_nonce(full_nonce.to_u256());
            let nonce = tx_nonce.as_u64();

            let account_code_key = get_account_code_key(address);
            let (code_hash, code) = data
                .sload(account_code_account, account_code_key)
                .ok()
                .and_then(|zk_bytecode_hash| {
                    ctx.dual_compiled_contracts
                        .find_by_zk_bytecode_hash(zk_bytecode_hash.to_h256())
                        .map(|contract| {
                            (
                                contract.evm_bytecode_hash,
                                Some(Bytecode::new_raw(Bytes::from(
                                    contract.evm_deployed_bytecode.clone(),
                                ))),
                            )
                        })
                })
                .unwrap_or_else(|| (KECCAK_EMPTY, None));

            let account = journaled_account(data, address).expect("failed to load account");
            let _ = std::mem::replace(&mut account.info.balance, balance);
            let _ = std::mem::replace(&mut account.info.nonce, nonce);

            if test_contract.map(|addr| addr == address).unwrap_or_default() {
                tracing::trace!(?address, "ignoring code translation for test contract");
            } else {
                account.info.code_hash = code_hash;
                account.info.code.clone_from(&code);
            }
        }
    }

    /// Switch to ZK-VM and translate block info, balances, nonces and deployed codes for persistent
    /// accounts
    pub fn select_zk_vm(
        &self,
        ctx: &mut ZksyncCheatcodeInspectorStrategyContext,
        data: InnerEcx<'_, '_, '_>,
        new_env: Option<&Env>,
    ) {
        if ctx.using_zk_vm {
            tracing::info!("already in ZK-VM");
            return
        }

        tracing::info!("switching to ZK-VM");
        ctx.using_zk_vm = true;

        let env = new_env.unwrap_or(data.env.as_ref());

        let mut system_storage: HashMap<U256, EvmStorageSlot> = Default::default();
        let block_info_key = CURRENT_VIRTUAL_BLOCK_INFO_POSITION.to_ru256();
        let block_info =
            pack_block_info(env.block.number.as_limbs()[0], env.block.timestamp.as_limbs()[0]);
        system_storage.insert(block_info_key, EvmStorageSlot::new(block_info.to_ru256()));

        let mut l2_eth_storage: HashMap<U256, EvmStorageSlot> = Default::default();
        let mut nonce_storage: HashMap<U256, EvmStorageSlot> = Default::default();
        let mut account_code_storage: HashMap<U256, EvmStorageSlot> = Default::default();
        let mut known_codes_storage: HashMap<U256, EvmStorageSlot> = Default::default();
        let mut deployed_codes: HashMap<Address, AccountInfo> = Default::default();

        let test_contract = data.db.get_test_contract_address();
        for address in data.db.persistent_accounts().into_iter().chain([data.env.tx.caller]) {
            info!(?address, "importing to zk state");

            let account = journaled_account(data, address).expect("failed to load account");
            let info = &account.info;

            let balance_key = get_balance_key(address);
            l2_eth_storage.insert(balance_key, EvmStorageSlot::new(info.balance));

            // TODO we need to find a proper way to handle deploy nonces instead of replicating
            let full_nonce = nonces_to_full_nonce(info.nonce.into(), info.nonce.into());

            let nonce_key = get_nonce_key(address);
            nonce_storage.insert(nonce_key, EvmStorageSlot::new(full_nonce.to_ru256()));

            if test_contract.map(|test_address| address == test_address).unwrap_or_default() {
                // avoid migrating test contract code
                tracing::trace!(?address, "ignoring code translation for test contract");
                continue;
            }

            if let Some(contract) = ctx.dual_compiled_contracts.iter().find(|contract| {
                info.code_hash != KECCAK_EMPTY && info.code_hash == contract.evm_bytecode_hash
            }) {
                account_code_storage.insert(
                    get_account_code_key(address),
                    EvmStorageSlot::new(contract.zk_bytecode_hash.to_ru256()),
                );
                known_codes_storage
                    .insert(contract.zk_bytecode_hash.to_ru256(), EvmStorageSlot::new(U256::ZERO));

                let code_hash = B256::from_slice(contract.zk_bytecode_hash.as_bytes());
                deployed_codes.insert(
                    address,
                    AccountInfo {
                        balance: info.balance,
                        nonce: info.nonce,
                        code_hash,
                        code: Some(Bytecode::new_raw(Bytes::from(
                            contract.zk_deployed_bytecode.clone(),
                        ))),
                    },
                );
            } else {
                tracing::debug!(code_hash = ?info.code_hash, ?address, "no zk contract found")
            }
        }

        let system_addr = SYSTEM_CONTEXT_ADDRESS.to_address();
        let system_account = journaled_account(data, system_addr).expect("failed to load account");
        system_account.storage.extend(system_storage.clone());

        let balance_addr = L2_BASE_TOKEN_ADDRESS.to_address();
        let balance_account =
            journaled_account(data, balance_addr).expect("failed to load account");
        balance_account.storage.extend(l2_eth_storage.clone());

        let nonce_addr = NONCE_HOLDER_ADDRESS.to_address();
        let nonce_account = journaled_account(data, nonce_addr).expect("failed to load account");
        nonce_account.storage.extend(nonce_storage.clone());

        let account_code_addr = ACCOUNT_CODE_STORAGE_ADDRESS.to_address();
        let account_code_account =
            journaled_account(data, account_code_addr).expect("failed to load account");
        account_code_account.storage.extend(account_code_storage.clone());

        let known_codes_addr = KNOWN_CODES_STORAGE_ADDRESS.to_address();
        let known_codes_account =
            journaled_account(data, known_codes_addr).expect("failed to load account");
        known_codes_account.storage.extend(known_codes_storage.clone());

        for (address, info) in deployed_codes {
            let account = journaled_account(data, address).expect("failed to load account");
            let _ = std::mem::replace(&mut account.info.balance, info.balance);
            let _ = std::mem::replace(&mut account.info.nonce, info.nonce);
            account.info.code_hash = info.code_hash;
            account.info.code.clone_from(&info.code);
        }
    }
}

/// Setting for migrating the database to zkEVM storage when starting in ZKsync mode.
/// The migration is performed on the DB via the inspector so must only be performed once.
#[derive(Debug, Default, Clone)]
pub enum ZkStartupMigration {
    /// Defer database migration to a later execution point.
    ///
    /// This is required as we need to wait for some baseline deployments
    /// to occur before the test/script execution is performed.
    #[default]
    Defer,
    /// Allow database migration.
    Allow,
    /// Database migration has already been performed.
    Done,
}

impl ZkStartupMigration {
    /// Check if startup migration is allowed. Migration is disallowed if it's to be deferred or has
    /// already been performed.
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }

    /// Allow migrating the the DB to zkEVM storage.
    pub fn allow(&mut self) {
        *self = Self::Allow
    }

    /// Mark the migration as completed. It must not be performed again.
    pub fn done(&mut self) {
        *self = Self::Done
    }
}

/// Create ZKsync strategy for [CheatcodeInspectorStrategy].
pub trait ZksyncCheatcodeInspectorStrategyBuilder {
    /// Create new ZKsync strategy.
    fn new_zksync(dual_compiled_contracts: DualCompiledContracts, zk_env: ZkEnv) -> Self;
}

impl ZksyncCheatcodeInspectorStrategyBuilder for CheatcodeInspectorStrategy {
    fn new_zksync(dual_compiled_contracts: DualCompiledContracts, zk_env: ZkEnv) -> Self {
        Self {
            runner: Box::new(ZksyncCheatcodeInspectorStrategyRunner::default()),
            context: Box::new(ZksyncCheatcodeInspectorStrategyContext::new(
                dual_compiled_contracts,
                zk_env,
            )),
        }
    }
}

fn get_artifact_code(
    dual_compiled_contracts: &DualCompiledContracts,
    using_zk_vm: bool,
    config: &Arc<CheatsConfig>,
    path: &str,
    deployed: bool,
) -> Result<Bytes> {
    let path = if path.ends_with(".json") {
        PathBuf::from(path)
    } else {
        let mut parts = path.split(':');

        let mut file = None;
        let mut contract_name = None;
        let mut version = None;

        let path_or_name = parts.next().unwrap();
        if path_or_name.contains('.') {
            file = Some(PathBuf::from(path_or_name));
            if let Some(name_or_version) = parts.next() {
                if name_or_version.contains('.') {
                    version = Some(name_or_version);
                } else {
                    contract_name = Some(name_or_version);
                    version = parts.next();
                }
            }
        } else {
            contract_name = Some(path_or_name);
            version = parts.next();
        }

        let version = if let Some(version) = version {
            Some(Version::parse(version).map_err(|e| fmt_err!("failed parsing version: {e}"))?)
        } else {
            None
        };

        // Use available artifacts list if present
        if let Some(artifacts) = &config.available_artifacts {
            let filtered = artifacts
                .iter()
                .filter(|(id, _)| {
                    // name might be in the form of "Counter.0.8.23"
                    let id_name = id.name.split('.').next().unwrap();

                    if let Some(path) = &file {
                        if !id.source.ends_with(path) {
                            return false;
                        }
                    }
                    if let Some(name) = contract_name {
                        if id_name != name {
                            return false;
                        }
                    }
                    if let Some(ref version) = version {
                        if id.version.minor != version.minor ||
                            id.version.major != version.major ||
                            id.version.patch != version.patch
                        {
                            return false;
                        }
                    }
                    true
                })
                .collect::<Vec<_>>();

            let artifact = match &filtered[..] {
                [] => Err(fmt_err!("no matching artifact found")),
                [artifact] => Ok(artifact),
                filtered => {
                    // If we find more than one artifact, we need to filter by contract type
                    // depending on whether we are using the zkvm or evm
                    filtered
                        .iter()
                        .find(|(id, _)| {
                            let contract_type =
                                dual_compiled_contracts.get_contract_type_by_artifact(id);
                            match contract_type {
                                Some(ContractType::ZK) => using_zk_vm,
                                Some(ContractType::EVM) => !using_zk_vm,
                                None => false,
                            }
                        })
                        .or_else(|| {
                            // If we know the current script/test contract solc version, try to
                            // filter by it
                            config.running_version.as_ref().and_then(|version| {
                                filtered.iter().find(|(id, _)| id.version == *version)
                            })
                        })
                        .ok_or_else(|| fmt_err!("multiple matching artifacts found"))
                }
            }?;

            let maybe_bytecode = if deployed {
                artifact.1.deployed_bytecode().cloned()
            } else {
                artifact.1.bytecode().cloned()
            };

            return maybe_bytecode
                .ok_or_else(|| fmt_err!("no bytecode for contract; is it abstract or unlinked?"));
        } else {
            let path_in_artifacts =
                match (file.map(|f| f.to_string_lossy().to_string()), contract_name) {
                    (Some(file), Some(contract_name)) => {
                        PathBuf::from(format!("{file}/{contract_name}.json"))
                    }
                    (None, Some(contract_name)) => {
                        PathBuf::from(format!("{contract_name}.sol/{contract_name}.json"))
                    }
                    (Some(file), None) => {
                        let name = file.replace(".sol", "");
                        PathBuf::from(format!("{file}/{name}.json"))
                    }
                    _ => bail!("invalid artifact path"),
                };

            config.paths.artifacts.join(path_in_artifacts)
        }
    };

    let path = config.ensure_path_allowed(path, FsAccessKind::Read)?;
    let data = fs::read_to_string(path)?;
    let artifact = serde_json::from_str::<ContractObject>(&data)?;
    let maybe_bytecode = if deployed { artifact.deployed_bytecode } else { artifact.bytecode };
    maybe_bytecode.ok_or_else(|| fmt_err!("no bytecode for contract; is it abstract or unlinked?"))
}

fn get_context(
    ctx: &mut dyn CheatcodeInspectorStrategyContext,
) -> &mut ZksyncCheatcodeInspectorStrategyContext {
    ctx.as_any_mut().downcast_mut().expect("expected ZksyncCheatcodeInspectorStrategyContext")
}
