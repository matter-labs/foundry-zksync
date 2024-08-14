use std::collections::HashMap;

use alloy_primitives::{Address, U256};

/// Records storage slots reads and writes.
#[derive(Clone, Debug, Default)]
pub struct RecordAccess {
    /// Storage slots reads.
    pub reads: HashMap<Address, Vec<U256>>,
    /// Storage slots writes.
    pub writes: HashMap<Address, Vec<U256>>,
}

impl RecordAccess {
    /// Records a read access to a storage slot.
    pub fn record_read(&mut self, target: Address, slot: U256) {
        self.reads.entry(target).or_default().push(slot);
    }

    /// Records a write access to a storage slot.
    ///
    /// This also records a read internally as `SSTORE` does an implicit `SLOAD`.
    pub fn record_write(&mut self, target: Address, slot: U256) {
        self.record_read(target, slot);
        self.writes.entry(target).or_default().push(slot);
    }
}
