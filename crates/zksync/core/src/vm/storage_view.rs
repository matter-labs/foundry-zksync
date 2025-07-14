use std::{cell::RefCell, collections::HashMap, fmt, rc::Rc};

use alloy_primitives::{Address, U256};
use zksync_types::{
    utils::storage_key_for_eth_balance, StorageKey, StorageValue, ACCOUNT_CODE_STORAGE_ADDRESS,
    H160, H256,
};
use zksync_vm_interface::storage::{ReadStorage, WriteStorage};

use crate::convert::{ConvertAddress, ConvertH160, ConvertH256};

use super::storage_recorder::{CallAddresses, CallType, StorageAccessRecorder};

/// `StorageView` is a buffer for `StorageLog`s between storage and transaction execution code.
/// In order to commit transactions logs should be submitted to the underlying storage
/// after a transaction is executed.
///
/// When executing transactions as a part of miniblock / L1 batch creation,
/// a single `StorageView` is used for the entire L1 batch.
/// One `StorageView` must not be used for multiple L1 batches;
/// otherwise, [`Self::is_write_initial()`] will return incorrect values because of the caching.
///
/// When executing transactions in the API sandbox, a dedicated view is used for each transaction;
/// the only shared part is the read storage keys cache.
#[derive(Debug)]
pub(crate) struct StorageView<S> {
    pub(crate) storage_handle: S,
    /// Used for caching and to get the list/count of modified keys
    pub(crate) modified_storage_keys: HashMap<StorageKey, StorageValue>,
    /// Used purely for caching
    pub(crate) read_storage_keys: HashMap<StorageKey, StorageValue>,
    /// Cache for `contains_key()` checks. The cache is only valid within one L1 batch execution.
    initial_writes_cache: HashMap<StorageKey, bool>,
    /// The tx caller.
    caller: H160,
    /// Call tracker for recording storage accesses.
    /// Track `FarCalls`s to allow matching them with their respective `Ret` opcodes.
    /// zkEVM erases the `msg.sender` and `code_address` for certain calls like to MsgSimulator,
    /// so we track them using this strategy to retain this information.
    call_tracker: Vec<CallAddresses>,
}

impl<S: ReadStorage + fmt::Debug> StorageView<S> {
    /// Creates a new storage view based on the underlying storage.
    pub(crate) fn new(
        storage_handle: S,
        modified_storage_keys: HashMap<StorageKey, StorageValue>,
        caller: H160,
    ) -> Self {
        Self {
            storage_handle,
            modified_storage_keys,
            read_storage_keys: HashMap::new(),
            initial_writes_cache: HashMap::new(),
            caller,
            call_tracker: Default::default(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn clean_cache(&mut self) {
        self.modified_storage_keys = Default::default();
        self.read_storage_keys = Default::default();
        self.initial_writes_cache = Default::default();
    }

    fn get_value_no_log(&mut self, key: &StorageKey) -> StorageValue {
        let cached_value =
            self.modified_storage_keys.get(key).or_else(|| self.read_storage_keys.get(key));
        cached_value.copied().unwrap_or_else(|| {
            let value = self.storage_handle.read_value(key);
            self.read_storage_keys.insert(*key, value);
            value
        })
    }

    /// Make a Rc RefCell ptr to the storage
    pub(crate) fn into_rc_ptr(self) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(self))
    }
}

impl<S: ReadStorage + fmt::Debug> ReadStorage for StorageView<S> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        let value = self.get_value_no_log(key);

        // We override the caller's account code storage to allow for calls
        if key.address() == &ACCOUNT_CODE_STORAGE_ADDRESS && key.key() == &self.caller.to_h256() {
            let value = StorageValue::zero();
            tracing::trace!(
                hashed_key = ?key.hashed_key(),
                ?value,
                address = ?key.address(),
                key = ?key.key(),
                "override read value",
            );

            return value
        }

        tracing::trace!(
            hashed_key = ?key.hashed_key(),
            ?value,
            address = ?key.address(),
            key = ?key.key(),
            "read value",
        );

        value
    }

    /// Only keys contained in the underlying storage will return `false`. If a key was
    /// inserted using [`Self::set_value()`], it will still return `true`.
    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        if let Some(&is_write_initial) = self.initial_writes_cache.get(key) {
            is_write_initial
        } else {
            let is_write_initial = self.storage_handle.is_write_initial(key);
            self.initial_writes_cache.insert(*key, is_write_initial);
            is_write_initial
        }
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.storage_handle.load_factory_dep(hash)
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        self.storage_handle.get_enumeration_index(key)
    }
}

impl<S: ReadStorage + fmt::Debug + StorageAccessRecorder> WriteStorage for StorageView<S> {
    fn read_storage_keys(&self) -> &HashMap<StorageKey, StorageValue> {
        &self.read_storage_keys
    }

    fn set_value(&mut self, key: StorageKey, value: StorageValue) -> StorageValue {
        let original = self.get_value_no_log(&key);

        tracing::trace!(
            hashed_key = ?key.hashed_key(),
            ?value,
            ?original,
            address = ?key.address(),
            key = ?key.key(),
            "write value",
        );

        self.storage_handle.record_write(&key, original, value);

        self.modified_storage_keys.insert(key, value);

        original
    }

    fn modified_storage_keys(&self) -> &HashMap<StorageKey, StorageValue> {
        &self.modified_storage_keys
    }

    fn missed_storage_invocations(&self) -> usize {
        0
    }
}

/// Allows recording accesses on the storage view by CAlls and CREATEs.
pub trait StorageViewRecorder {
    fn start_recording(&mut self);
    fn stop_recording(&mut self);
    fn record_call_start(
        &mut self,
        is_mimic: bool,
        call_type: CallType,
        accessor: Address,
        account: Address,
        data: Vec<u8>,
        value: U256,
    );
    fn record_call_end(&mut self);
}

impl<S: ReadStorage + fmt::Debug + StorageAccessRecorder> StorageViewRecorder for StorageView<S> {
    fn start_recording(&mut self) {
        self.storage_handle.start_recording();
    }

    fn stop_recording(&mut self) {
        self.storage_handle.stop_recording();
    }

    fn record_call_start(
        &mut self,
        is_mimic: bool,
        call_type: CallType,
        accessor: Address,
        account: Address,
        data: Vec<u8>,
        value: U256,
    ) {
        // if a call is mimic with a value, then it's a call with transfer and the balance is
        // already updated via call to MsgValueSimulator, so we need to account for that here.
        let balance = if is_mimic && !value.is_zero() {
            self.read_value(&storage_key_for_eth_balance(&account.to_h160()))
                .to_ru256()
                .saturating_sub(value)
        } else {
            self.read_value(&storage_key_for_eth_balance(&account.to_h160())).to_ru256()
        };

        self.call_tracker.push(CallAddresses { account, accessor });
        self.storage_handle.record_call_start(call_type, accessor, account, balance, data, value);
    }

    fn record_call_end(&mut self) {
        let CallAddresses { account, accessor } =
            self.call_tracker.pop().expect("unexpected request for call addresses; none on stack");
        let new_balance =
            self.read_value(&storage_key_for_eth_balance(&account.to_h160())).to_ru256();
        self.storage_handle.record_call_end(account, accessor, new_balance);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use alloy_primitives::Address as rAddress;
    use zksync_types::{AccountTreeId, Address, H256};
    use zksync_vm_interface::storage::InMemoryStorage;

    impl StorageAccessRecorder for &InMemoryStorage {
        fn start_recording(&mut self) {}
        fn stop_recording(&mut self) {}
        fn record_read(&mut self, _key: &StorageKey, _value: H256) {}
        fn record_write(&mut self, _key: &StorageKey, _old_value: H256, _new_value: H256) {}
        fn record_call_start(
            &mut self,
            _call_type: CallType,
            _accessor: rAddress,
            _account: rAddress,
            _balance: U256,
            _data: Vec<u8>,
            _value: U256,
        ) {
        }
        fn record_call_end(&mut self, _accessor: rAddress, _account: rAddress, _new_balance: U256) {
        }
    }

    #[test]
    fn test_storage_access() {
        let account: AccountTreeId = AccountTreeId::new(Address::from([0xfe; 20]));
        let key = H256::from_low_u64_be(61);
        let value = H256::from_low_u64_be(73);
        let key = StorageKey::new(account, key);

        let mut raw_storage = InMemoryStorage::default();
        let mut storage_view =
            StorageView::new(&raw_storage, Default::default(), Default::default());

        let default_value = storage_view.read_value(&key);
        assert_eq!(default_value, H256::zero());

        let prev_value = storage_view.set_value(key, value);
        assert_eq!(prev_value, H256::zero());
        assert_eq!(storage_view.read_value(&key), value);
        assert!(storage_view.is_write_initial(&key)); // key was inserted during the view lifetime

        raw_storage.set_value(key, value);
        let mut storage_view =
            StorageView::new(&raw_storage, Default::default(), Default::default());

        assert_eq!(storage_view.read_value(&key), value);
        assert!(!storage_view.is_write_initial(&key)); // `key` is present in `raw_storage`

        let new_value = H256::from_low_u64_be(74);
        storage_view.set_value(key, new_value);
        assert_eq!(storage_view.read_value(&key), new_value);

        let new_key = StorageKey::new(account, H256::from_low_u64_be(62));
        storage_view.set_value(new_key, new_value);
        assert_eq!(storage_view.read_value(&new_key), new_value);
        assert!(storage_view.is_write_initial(&new_key));
    }
}
