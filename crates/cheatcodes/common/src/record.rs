use std::collections::HashMap;

use alloy_primitives::{Address, U256};
use serde::Serialize;

/// JSON-serializable log entry for `getRecordedLogsJson`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogJson {
    /// The topics of the log, including the signature, if any.
    pub topics: Vec<String>,
    /// The raw data of the log, hex-encoded with 0x prefix.
    pub data: String,
    /// The address of the log's emitter.
    pub emitter: String,
}

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

    /// Clears the recorded reads and writes.
    pub fn clear(&mut self) {
        // Also frees memory.
        *self = Default::default();
    }
}
