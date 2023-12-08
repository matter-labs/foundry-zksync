use era_test_node::fork::{ForkSource, ForkStorage};
use era_test_node::utils::bytecode_to_factory_dep;
use ethers::utils::to_checksum;
use ethers::{abi::AbiDecode, prelude::abigen};
use itertools::Itertools;
use multivm::interface::dyn_tracers::vm_1_3_3::DynTracer;
use multivm::interface::tracer::TracerExecutionStatus;
use multivm::vm_refunds_enhancement::{
    BootloaderState, HistoryMode, SimpleMemory, VmTracer, ZkSyncVmState,
};
use multivm::zk_evm_1_3_1::zkevm_opcode_defs::RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER;
use multivm::zk_evm_1_3_3::tracing::{BeforeExecutionData, VmLocalStateData};
use multivm::zk_evm_1_3_3::vm_state::PrimitiveValue;
use std::collections::HashMap;
use std::fmt::Debug;
use zk_evm::zkevm_opcode_defs::{FatPointer, Opcode, CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER};
use zksync_basic_types::{AccountTreeId, Address, H160, H256, U256};
use zksync_state::{ReadStorage, StoragePtr, StorageView};
use zksync_types::{
    block::{pack_block_info, unpack_block_info},
    get_code_key, get_nonce_key,
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
    StorageKey,
};
use zksync_types::{LogQuery, Timestamp};
use zksync_utils::{h256_to_u256, u256_to_h256};

// address(uint160(uint256(keccak256('hevm cheat code'))))
const CHEATCODE_ADDRESS: H160 = H160([
    113, 9, 112, 158, 207, 169, 26, 128, 98, 111, 243, 152, 157, 104, 246, 127, 91, 29, 209, 45,
]);

const INTERNAL_CONTRACT_ADDRESSES: [H160; 20] = [
    zksync_types::BOOTLOADER_ADDRESS,
    zksync_types::ACCOUNT_CODE_STORAGE_ADDRESS,
    zksync_types::NONCE_HOLDER_ADDRESS,
    zksync_types::KNOWN_CODES_STORAGE_ADDRESS,
    zksync_types::IMMUTABLE_SIMULATOR_STORAGE_ADDRESS,
    zksync_types::CONTRACT_DEPLOYER_ADDRESS,
    zksync_types::CONTRACT_FORCE_DEPLOYER_ADDRESS,
    zksync_types::L1_MESSENGER_ADDRESS,
    zksync_types::MSG_VALUE_SIMULATOR_ADDRESS,
    zksync_types::KECCAK256_PRECOMPILE_ADDRESS,
    zksync_types::L2_ETH_TOKEN_ADDRESS,
    zksync_types::SYSTEM_CONTEXT_ADDRESS,
    zksync_types::BOOTLOADER_UTILITIES_ADDRESS,
    zksync_types::EVENT_WRITER_ADDRESS,
    zksync_types::COMPRESSOR_ADDRESS,
    zksync_types::COMPLEX_UPGRADER_ADDRESS,
    zksync_types::ECRECOVER_PRECOMPILE_ADDRESS,
    zksync_types::SHA256_PRECOMPILE_ADDRESS,
    zksync_types::MINT_AND_BURN_ADDRESS,
    H160::zero(),
];

type ForkStorageView<S> = StorageView<ForkStorage<S>>;

abigen!(
    CheatcodeContract,
    r#"[
        function addr(uint256 privateKey)
        function deal(address who, uint256 newBalance)
        function etch(address who, bytes calldata code)
        function getNonce(address account)
        function load(address account, bytes32 slot)
        function roll(uint256 blockNumber)
        function serializeAddress(string objectKey, string valueKey, address value)
        function serializeBool(string objectKey, string valueKey, bool value)
        function serializeUint(string objectKey, string valueKey, uint256 value)
        function setNonce(address account, uint64 nonce)
        function store(address account, bytes32 slot, bytes32 value)
        function startPrank(address sender)
        function startPrank(address sender, address origin)
        function stopPrank()
        function toString(address value)
        function toString(bool value)
        function toString(uint256 value)
        function toString(int256 value)
        function toString(bytes32 value)
        function toString(bytes value)
        function warp(uint256 timestamp)
    ]"#
);

#[derive(Debug, Default, Clone)]
pub struct CheatcodeTracer {
    one_time_actions: Vec<FinishCycleOneTimeActions>,
    permanent_actions: FinishCyclePermanentActions,
    return_data: Option<Vec<U256>>,
    return_ptr: Option<FatPointer>,
    near_calls: usize,
    serialized_objects: HashMap<String, String>,
}

#[derive(Debug, Clone)]
enum FinishCycleOneTimeActions {
    StorageWrite {
        key: StorageKey,
        read_value: H256,
        write_value: H256,
    },
    StoreFactoryDep {
        hash: U256,
        bytecode: Vec<U256>,
    },
}

#[derive(Debug, Default, Clone)]
struct FinishCyclePermanentActions {
    start_prank: Option<StartPrankOpts>,
}

#[derive(Debug, Clone)]
struct StartPrankOpts {
    sender: H160,
    origin: Option<H256>,
}

impl<S: std::fmt::Debug + ForkSource, H: HistoryMode> DynTracer<ForkStorageView<S>, SimpleMemory<H>>
    for CheatcodeTracer
{
    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        memory: &SimpleMemory<H>,
        storage: StoragePtr<ForkStorageView<S>>,
    ) {
        if let Opcode::NearCall(_call) = data.opcode.variant.opcode {
            let current = state.vm_local_state.callstack.current;
            if current.this_address != CHEATCODE_ADDRESS {
                return;
            }
            if current.code_page.0 == 0 || current.ergs_remaining == 0 {
                tracing::error!("cheatcode triggered, but no calldata or ergs available");
                return;
            }
            tracing::info!("near call: cheatcode triggered");
            let calldata = {
                let ptr = state.vm_local_state.registers
                    [CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER as usize];
                assert!(ptr.is_pointer);
                let fat_data_pointer = FatPointer::from_u256(ptr.value);
                memory.read_unaligned_bytes(
                    fat_data_pointer.memory_page as usize,
                    fat_data_pointer.start as usize,
                    fat_data_pointer.length as usize,
                )
            };

            // try to dispatch the cheatcode
            if let Ok(call) = CheatcodeContractCalls::decode(calldata.clone()) {
                self.dispatch_cheatcode(state, data, memory, storage, call)
            } else {
                tracing::error!(
                    "Failed to decode cheatcode calldata (near call): {}",
                    hex::encode(calldata),
                );
            }
        }
    }

    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: multivm::zk_evm_1_3_3::tracing::AfterExecutionData,
        _memory: &SimpleMemory<H>,
        _storage: StoragePtr<ForkStorageView<S>>,
    ) {
        if self.return_data.is_some() {
            if let Opcode::Ret(_call) = data.opcode.variant.opcode {
                if self.near_calls == 0 {
                    let ptr = state.vm_local_state.registers
                        [RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER as usize];
                    let fat_data_pointer = FatPointer::from_u256(ptr.value);
                    self.return_ptr = Some(fat_data_pointer);
                } else {
                    self.near_calls = self.near_calls.saturating_sub(1);
                }
            }
        }

        if let Opcode::NearCall(_call) = data.opcode.variant.opcode {
            if self.return_data.is_some() {
                self.near_calls += 1;
            }
        }
    }
}

impl<S: std::fmt::Debug + ForkSource, H: HistoryMode> VmTracer<ForkStorageView<S>, H>
    for CheatcodeTracer
{
    fn finish_cycle(
        &mut self,
        state: &mut ZkSyncVmState<ForkStorageView<S>, H>,
        _bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        while let Some(action) = self.one_time_actions.pop() {
            match action {
                FinishCycleOneTimeActions::StorageWrite {
                    key,
                    read_value,
                    write_value,
                } => {
                    state.storage.write_value(LogQuery {
                        timestamp: Timestamp(state.local_state.timestamp),
                        tx_number_in_block: state.local_state.tx_number_in_block,
                        aux_byte: Default::default(),
                        shard_id: Default::default(),
                        address: *key.address(),
                        key: h256_to_u256(*key.key()),
                        read_value: h256_to_u256(read_value),
                        written_value: h256_to_u256(write_value),
                        rw_flag: true,
                        rollback: false,
                        is_service: false,
                    });
                }
                FinishCycleOneTimeActions::StoreFactoryDep { hash, bytecode } => {
                    state.decommittment_processor.populate(
                        vec![(hash, bytecode)],
                        Timestamp(state.local_state.timestamp),
                    )
                }
            }
        }

        // Set return data, if any
        if let Some(mut fat_pointer) = self.return_ptr.take() {
            let timestamp = Timestamp(state.local_state.timestamp);

            let elements = self.return_data.take().unwrap();
            fat_pointer.length = (elements.len() as u32) * 32;
            state.local_state.registers[RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER as usize] =
                PrimitiveValue {
                    value: fat_pointer.to_u256(),
                    is_pointer: true,
                };
            state.memory.populate_page(
                fat_pointer.memory_page as usize,
                elements.into_iter().enumerate().collect_vec(),
                timestamp,
            );
        }

        // Sets the sender address for startPrank cheatcode
        if let Some(start_prank_call) = &self.permanent_actions.start_prank {
            let this_address = state.local_state.callstack.current.this_address;
            if !INTERNAL_CONTRACT_ADDRESSES.contains(&this_address) {
                state.local_state.callstack.current.msg_sender = start_prank_call.sender;
            }
        }

        TracerExecutionStatus::Continue
    }
}

impl CheatcodeTracer {
    pub fn new() -> Self {
        CheatcodeTracer {
            one_time_actions: vec![],
            permanent_actions: FinishCyclePermanentActions { start_prank: None },
            near_calls: 0,
            return_data: None,
            return_ptr: None,
            serialized_objects: HashMap::new(),
        }
    }

    pub fn dispatch_cheatcode<S: std::fmt::Debug + ForkSource, H: HistoryMode>(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: BeforeExecutionData,
        _memory: &SimpleMemory<H>,
        storage: StoragePtr<ForkStorageView<S>>,
        call: CheatcodeContractCalls,
    ) {
        use CheatcodeContractCalls::*;
        match call {
            Addr(AddrCall { private_key }) => {
                tracing::info!("👷 Getting address for private key");
                let Ok(address) = zksync_types::PackedEthSignature::address_from_private_key(
                    &u256_to_h256(private_key),
                ) else {
                    tracing::error!("Failed generating address for private key");
                    return;
                };
                self.return_data = Some(vec![h256_to_u256(address.into())]);
            }
            Deal(DealCall { who, new_balance }) => {
                tracing::info!("👷 Setting balance for {who:?} to {new_balance}");
                self.write_storage(
                    storage_key_for_eth_balance(&who),
                    u256_to_h256(new_balance),
                    &mut storage.borrow_mut(),
                );
            }
            Etch(EtchCall { who, code }) => {
                tracing::info!("👷 Setting address code for {who:?}");
                let code_key = get_code_key(&who);
                let (hash, code) = bytecode_to_factory_dep(code.0.into());
                self.store_factory_dep(hash, code);
                self.write_storage(code_key, u256_to_h256(hash), &mut storage.borrow_mut());
            }
            GetNonce(GetNonceCall { account }) => {
                tracing::info!("👷 Getting nonce for {account:?}");
                let mut storage = storage.borrow_mut();
                let nonce_key = get_nonce_key(&account);
                let full_nonce = storage.read_value(&nonce_key);
                let (account_nonce, _) = decompose_full_nonce(h256_to_u256(full_nonce));
                tracing::info!(
                    "👷 Nonces for account {:?} are {}",
                    account,
                    account_nonce.as_u64()
                );
                tracing::info!("👷 Setting returndata",);
                tracing::info!("👷 Returndata is {:?}", account_nonce);
                self.return_data = Some(vec![account_nonce]);
            }
            Load(LoadCall { account, slot }) => {
                tracing::info!(
                    "👷 Getting storage slot {:?} for account {:?}",
                    slot,
                    account
                );
                let key = StorageKey::new(AccountTreeId::new(account), H256(slot));
                let mut storage = storage.borrow_mut();
                let value = storage.read_value(&key);
                self.return_data = Some(vec![h256_to_u256(value)]);
            }
            Roll(RollCall { block_number }) => {
                tracing::info!("👷 Setting block number to {}", block_number);

                let key = StorageKey::new(
                    AccountTreeId::new(zksync_types::SYSTEM_CONTEXT_ADDRESS),
                    zksync_types::CURRENT_VIRTUAL_BLOCK_INFO_POSITION,
                );
                let mut storage = storage.borrow_mut();
                let (_, block_timestamp) =
                    unpack_block_info(h256_to_u256(storage.read_value(&key)));
                self.write_storage(
                    key,
                    u256_to_h256(pack_block_info(block_number.as_u64(), block_timestamp)),
                    &mut storage,
                );
            }
            SerializeAddress(SerializeAddressCall {
                object_key,
                value_key,
                value,
            }) => {
                tracing::info!(
                    "👷 Serializing address {:?} with key {:?} to object {:?}",
                    value,
                    value_key,
                    object_key
                );
                let json_value = serde_json::json!({
                    value_key: value
                });

                //write to serialized_objects
                self.serialized_objects
                    .insert(object_key.clone(), json_value.to_string());

                let address = Address::from(value);
                let address_with_checksum = to_checksum(&address, None);
                self.add_trimmed_return_data(address_with_checksum.as_bytes());
            }
            SerializeBool(SerializeBoolCall {
                object_key,
                value_key,
                value,
            }) => {
                tracing::info!(
                    "👷 Serializing bool {:?} with key {:?} to object {:?}",
                    value,
                    value_key,
                    object_key
                );
                let json_value = serde_json::json!({
                    value_key: value
                });

                self.serialized_objects
                    .insert(object_key.clone(), json_value.to_string());

                let bool_value = value.to_string();
                self.add_trimmed_return_data(bool_value.as_bytes());
            }
            SerializeUint(SerializeUintCall {
                object_key,
                value_key,
                value,
            }) => {
                tracing::info!(
                    "👷 Serializing uint256 {:?} with key {:?} to object {:?}",
                    value,
                    value_key,
                    object_key
                );
                let json_value = serde_json::json!({
                    value_key: value
                });

                self.serialized_objects
                    .insert(object_key.clone(), json_value.to_string());

                let uint_value = value.to_string();
                self.add_trimmed_return_data(uint_value.as_bytes());
            }
            SetNonce(SetNonceCall { account, nonce }) => {
                tracing::info!("👷 Setting nonce for {account:?} to {nonce}");
                let mut storage = storage.borrow_mut();
                let nonce_key = get_nonce_key(&account);
                let full_nonce = storage.read_value(&nonce_key);
                let (mut account_nonce, mut deployment_nonce) =
                    decompose_full_nonce(h256_to_u256(full_nonce));
                if account_nonce.as_u64() >= nonce {
                    tracing::error!(
                      "SetNonce cheatcode failed: Account nonce is already set to a higher value ({}, requested {})",
                      account_nonce,
                      nonce
                  );
                    return;
                }
                account_nonce = nonce.into();
                if deployment_nonce.as_u64() >= nonce {
                    tracing::error!(
                      "SetNonce cheatcode failed: Deployment nonce is already set to a higher value ({}, requested {})",
                      deployment_nonce,
                      nonce
                  );
                    return;
                }
                deployment_nonce = nonce.into();
                let enforced_full_nonce = nonces_to_full_nonce(account_nonce, deployment_nonce);
                tracing::info!(
                    "👷 Nonces for account {:?} have been set to {}",
                    account,
                    nonce
                );
                self.write_storage(nonce_key, u256_to_h256(enforced_full_nonce), &mut storage);
            }
            StartPrank(StartPrankCall { sender }) => {
                tracing::info!("👷 Starting prank to {sender:?}");
                self.permanent_actions.start_prank = Some(StartPrankOpts {
                    sender,
                    origin: None,
                });
            }
            StartPrankWithOrigin(StartPrankWithOriginCall { sender, origin }) => {
                tracing::info!("👷 Starting prank to {sender:?} with origin {origin:?}");
                let key = StorageKey::new(
                    AccountTreeId::new(zksync_types::SYSTEM_CONTEXT_ADDRESS),
                    zksync_types::SYSTEM_CONTEXT_TX_ORIGIN_POSITION,
                );
                let original_tx_origin = storage.borrow_mut().read_value(&key);
                self.write_storage(key, origin.into(), &mut storage.borrow_mut());

                self.permanent_actions.start_prank = Some(StartPrankOpts {
                    sender,
                    origin: Some(original_tx_origin),
                });
            }
            StopPrank(StopPrankCall) => {
                tracing::info!("👷 Stopping prank");

                if let Some(origin) = self
                    .permanent_actions
                    .start_prank
                    .as_ref()
                    .and_then(|v| v.origin)
                {
                    let key = StorageKey::new(
                        AccountTreeId::new(zksync_types::SYSTEM_CONTEXT_ADDRESS),
                        zksync_types::SYSTEM_CONTEXT_TX_ORIGIN_POSITION,
                    );
                    self.write_storage(key, origin, &mut storage.borrow_mut());
                }

                self.permanent_actions.start_prank = None;
            }
            Store(StoreCall {
                account,
                slot,
                value,
            }) => {
                tracing::info!(
                    "👷 Setting storage slot {:?} for account {:?} to {:?}",
                    slot,
                    account,
                    value
                );
                let mut storage = storage.borrow_mut();
                let key = StorageKey::new(AccountTreeId::new(account), H256(slot));
                self.write_storage(key, H256(value), &mut storage);
            }
            ToString0(ToString0Call { value }) => {
                tracing::info!("Converting address into string");
                let address = Address::from(value);
                let address_with_checksum = to_checksum(&address, None);
                self.add_trimmed_return_data(address_with_checksum.as_bytes());
            }
            ToString1(ToString1Call { value }) => {
                tracing::info!("Converting bool into string");
                let bool_value = value.to_string();
                self.add_trimmed_return_data(bool_value.as_bytes());
            }
            ToString2(ToString2Call { value }) => {
                tracing::info!("Converting uint256 into string");
                let uint_value = value.to_string();
                self.add_trimmed_return_data(uint_value.as_bytes());
            }
            ToString3(ToString3Call { value }) => {
                tracing::info!("Converting int256 into string");
                let int_value = value.to_string();
                self.add_trimmed_return_data(int_value.as_bytes());
            }
            ToString4(ToString4Call { value }) => {
                tracing::info!("Converting bytes32 into string");
                let bytes_value = format!("0x{}", hex::encode(value));
                self.add_trimmed_return_data(bytes_value.as_bytes());
            }
            ToString5(ToString5Call { value }) => {
                tracing::info!("Converting bytes into string");
                let bytes_value = format!("0x{}", hex::encode(value));
                self.add_trimmed_return_data(bytes_value.as_bytes());
            }
            Warp(WarpCall { timestamp }) => {
                tracing::info!("👷 Setting block timestamp {}", timestamp);

                let key = StorageKey::new(
                    AccountTreeId::new(zksync_types::SYSTEM_CONTEXT_ADDRESS),
                    zksync_types::CURRENT_VIRTUAL_BLOCK_INFO_POSITION,
                );
                let mut storage = storage.borrow_mut();
                let (block_number, _) = unpack_block_info(h256_to_u256(storage.read_value(&key)));
                self.write_storage(
                    key,
                    u256_to_h256(pack_block_info(block_number, timestamp.as_u64())),
                    &mut storage,
                );
            }
        };
    }

    fn store_factory_dep(&mut self, hash: U256, bytecode: Vec<U256>) {
        self.one_time_actions
            .push(FinishCycleOneTimeActions::StoreFactoryDep { hash, bytecode });
    }

    fn write_storage<S: std::fmt::Debug + ForkSource>(
        &mut self,
        key: StorageKey,
        write_value: H256,
        storage: &mut StorageView<ForkStorage<S>>,
    ) {
        self.one_time_actions
            .push(FinishCycleOneTimeActions::StorageWrite {
                key,
                read_value: storage.read_value(&key),
                write_value,
            });
    }

    fn add_trimmed_return_data(&mut self, data: &[u8]) {
        let data_length = data.len();
        let mut data: Vec<U256> = data
            .chunks(32)
            .map(|b| {
                // Copies the bytes into a 32 byte array
                // padding with zeros to the right if necessary
                let mut bytes = [0u8; 32];
                bytes[..b.len()].copy_from_slice(b);
                bytes.into()
            })
            .collect_vec();

        // Add the length of the data to the end of the return data
        data.push(data_length.into());

        self.return_data = Some(data);
    }
}
