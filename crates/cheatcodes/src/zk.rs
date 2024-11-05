use std::{cell::RefCell, rc::Rc};

use alloy_primitives::{Address, Bytes, B256, U256};
use foundry_evm_core::backend::DatabaseExt;
use foundry_zksync_compiler::DualCompiledContracts;
use foundry_zksync_core::{
    convert::{ConvertH160, ConvertH256, ConvertRU256, ConvertU256},
    get_account_code_key, get_balance_key, get_nonce_key,
};
use revm::{
    interpreter::{opcode, InstructionResult, Interpreter},
    primitives::{AccountInfo, Bytecode, Env, EvmStorageSlot, HashMap as rHashMap, KECCAK_EMPTY},
    EvmContext, InnerEvmContext,
};
use zksync_types::{
    block::{pack_block_info, unpack_block_info},
    utils::{decompose_full_nonce, nonces_to_full_nonce},
    ACCOUNT_CODE_STORAGE_ADDRESS, CURRENT_VIRTUAL_BLOCK_INFO_POSITION, KNOWN_CODES_STORAGE_ADDRESS,
    L2_BASE_TOKEN_ADDRESS, NONCE_HOLDER_ADDRESS, SYSTEM_CONTEXT_ADDRESS,
};

use crate::evm::journaled_account;

pub trait NetworkCheatcode: std::fmt::Debug {
    fn enabled(&self) -> bool;
    fn select_evm(&mut self, data: &mut InnerEvmContext<&mut dyn DatabaseExt>);
    fn select_custom(
        &mut self,
        data: &mut InnerEvmContext<&mut dyn DatabaseExt>,
        new_env: Option<&Env>,
    );
    fn handle_opcode(
        &self,
        interpreter: &mut Interpreter,
        ecx: &mut EvmContext<&mut dyn DatabaseExt>,
    ) -> bool;
}

#[derive(Debug)]
pub struct Zk {
    pub use_zk_vm: bool,
    pub dual_compiled_contracts: DualCompiledContracts,
}

impl NetworkCheatcode for Option<Rc<RefCell<Box<dyn NetworkCheatcode>>>> {
    fn enabled(&self) -> bool {
        self.as_ref().map_or(false, |n| n.borrow().enabled())
    }

    fn handle_opcode(
        &self,
        interpreter: &mut Interpreter,
        ecx: &mut EvmContext<&mut dyn DatabaseExt>,
    ) -> bool {
        self.as_ref().map_or(false, |n| n.borrow().handle_opcode(interpreter, ecx))
    }

    fn select_evm(&mut self, data: &mut InnerEvmContext<&mut dyn DatabaseExt>) {
        let _ = self.as_mut().map_or((), |n| n.borrow_mut().select_evm(data));
    }

    fn select_custom(
        &mut self,
        data: &mut InnerEvmContext<&mut dyn DatabaseExt>,
        new_env: Option<&Env>,
    ) {
        let _ = self.as_mut().map_or((), |n| n.borrow_mut().select_custom(data, new_env));
    }
}

impl NetworkCheatcode for Zk {
    fn enabled(&self) -> bool {
        self.use_zk_vm
    }

    fn handle_opcode(
        &self,
        interpreter: &mut Interpreter,
        ecx: &mut EvmContext<&mut dyn DatabaseExt>,
    ) -> bool {
        if self.enabled() {
            let address = match interpreter.current_opcode() {
                opcode::SELFBALANCE => interpreter.contract().target_address,
                opcode::BALANCE => {
                    if interpreter.stack.is_empty() {
                        interpreter.instruction_result = InstructionResult::StackUnderflow;
                        return true;
                    }

                    Address::from_word(B256::from(unsafe { interpreter.stack.pop_unsafe() }))
                }
                _ => return false,
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

            return true;
        }

        return false;
    }

    /// Switch to EVM and translate block info, balances, nonces and deployed codes for persistent
    /// accounts
    fn select_evm(&mut self, data: &mut InnerEvmContext<&mut dyn DatabaseExt>) {
        if !self.use_zk_vm {
            tracing::info!("already in EVM");
            return
        }

        tracing::info!("switching to EVM");
        self.use_zk_vm = false;

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
                    self.dual_compiled_contracts
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
    fn select_custom(
        &mut self,
        data: &mut InnerEvmContext<&mut dyn DatabaseExt>,
        new_env: Option<&Env>,
    ) {
        if self.use_zk_vm {
            tracing::info!("already in ZK-VM");
            return
        }

        tracing::info!("switching to ZK-VM");
        self.use_zk_vm = true;

        let env = new_env.unwrap_or(data.env.as_ref());

        let mut system_storage: rHashMap<U256, EvmStorageSlot> = Default::default();
        let block_info_key = CURRENT_VIRTUAL_BLOCK_INFO_POSITION.to_ru256();
        let block_info =
            pack_block_info(env.block.number.as_limbs()[0], env.block.timestamp.as_limbs()[0]);
        system_storage.insert(block_info_key, EvmStorageSlot::new(block_info.to_ru256()));

        let mut l2_eth_storage: rHashMap<U256, EvmStorageSlot> = Default::default();
        let mut nonce_storage: rHashMap<U256, EvmStorageSlot> = Default::default();
        let mut account_code_storage: rHashMap<U256, EvmStorageSlot> = Default::default();
        let mut known_codes_storage: rHashMap<U256, EvmStorageSlot> = Default::default();
        let mut deployed_codes: rHashMap<Address, AccountInfo> = Default::default();

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

            if let Some(contract) = self.dual_compiled_contracts.iter().find(|contract| {
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

        let test_contract = data.db.get_test_contract_address();
        for (address, info) in deployed_codes {
            let account = journaled_account(data, address).expect("failed to load account");
            let _ = std::mem::replace(&mut account.info.balance, info.balance);
            let _ = std::mem::replace(&mut account.info.nonce, info.nonce);
            if test_contract.map(|addr| addr == address).unwrap_or_default() {
                tracing::trace!(?address, "ignoring code translation for test contract");
            } else {
                account.info.code_hash = info.code_hash;
                account.info.code.clone_from(&info.code);
            }
        }
    }
}
