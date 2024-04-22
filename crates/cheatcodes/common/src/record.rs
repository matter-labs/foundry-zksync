use std::collections::HashMap;

use alloy_primitives::{Address, U256};

pub trait RecordAccesses: core::fmt::Debug {
    fn push_write(&mut self, key: Address, slot: U256);
    fn push_read(&mut self, key: Address, slot: U256);
}

//TODO: move the below back where it was
// plus the added implementations

/// Records storage slots reads and writes.
#[derive(Clone, Debug, Default)]
pub struct RecordAccess {
    /// Storage slots reads.
    pub reads: HashMap<Address, Vec<U256>>,
    /// Storage slots writes.
    pub writes: HashMap<Address, Vec<U256>>,
}

impl RecordAccesses for &mut Option<RecordAccess> {
    fn push_write(&mut self, key: Address, slot: U256) {
        <Option<_> as RecordAccesses>::push_write(self, key, slot)
    }

    fn push_read(&mut self, key: Address, slot: U256) {
        <Option<_> as RecordAccesses>::push_read(self, key, slot)
    }
}

impl RecordAccesses for Option<RecordAccess> {
    fn push_write(&mut self, key: Address, slot: U256) {
        if let Some(records) = self {
            records.writes.entry(key).or_default().push(slot);
        }
    }

    fn push_read(&mut self, key: Address, slot: U256) {
        if let Some(records) = self {
            records.reads.entry(key).or_default().push(slot);
        }
    }
}
