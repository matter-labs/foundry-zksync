use std::{cell::RefCell, collections::HashMap, fmt, rc::Rc};

use zksync_state::{ReadStorage, WriteStorage};
use zksync_types::{StorageKey, StorageValue, H256};

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
pub struct StorageView<S> {
    pub storage_handle: S,
    // Used for caching and to get the list/count of modified keys
    pub modified_storage_keys: HashMap<StorageKey, StorageValue>,
    // Used purely for caching
    pub read_storage_keys: HashMap<StorageKey, StorageValue>,
    // Cache for `contains_key()` checks. The cache is only valid within one L1 batch execution.
    initial_writes_cache: HashMap<StorageKey, bool>,
}

impl<S: ReadStorage + fmt::Debug> StorageView<S> {
    /// Creates a new storage view based on the underlying storage.
    pub fn new(storage_handle: S) -> Self {
        Self {
            storage_handle,
            modified_storage_keys: HashMap::new(),
            read_storage_keys: HashMap::new(),
            initial_writes_cache: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn clean_cache(&mut self) {
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
    pub fn into_rc_ptr(self) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(self))
    }
}

impl<S: ReadStorage + fmt::Debug> ReadStorage for StorageView<S> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        let value = self.get_value_no_log(key);

        tracing::trace!(
            "read value {:?} {:?} ({:?}/{:?})",
            key.hashed_key().0,
            value.0,
            key.address(),
            key.key()
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

impl<S: ReadStorage + fmt::Debug> WriteStorage for StorageView<S> {
    fn set_value(&mut self, key: StorageKey, value: StorageValue) -> StorageValue {
        let original = self.get_value_no_log(&key);

        tracing::trace!(
            "write value {:?} value: {:?} original value: {:?} ({:?}/{:?})",
            key.hashed_key().0,
            value,
            original,
            key.address(),
            key.key()
        );
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

#[cfg(test)]
mod test {
    use super::*;
    use zksync_state::InMemoryStorage;
    use zksync_types::{AccountTreeId, Address, H256};

    #[test]
    fn test_storage_access() {
        let account: AccountTreeId = AccountTreeId::new(Address::from([0xfe; 20]));
        let key = H256::from_low_u64_be(61);
        let value = H256::from_low_u64_be(73);
        let key = StorageKey::new(account, key);

        let mut raw_storage = InMemoryStorage::default();
        let mut storage_view = StorageView::new(&raw_storage);

        let default_value = storage_view.read_value(&key);
        assert_eq!(default_value, H256::zero());

        let prev_value = storage_view.set_value(key, value);
        assert_eq!(prev_value, H256::zero());
        assert_eq!(storage_view.read_value(&key), value);
        assert!(storage_view.is_write_initial(&key)); // key was inserted during the view lifetime

        raw_storage.set_value(key, value);
        let mut storage_view = StorageView::new(&raw_storage);

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
