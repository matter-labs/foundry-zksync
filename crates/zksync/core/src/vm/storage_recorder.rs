use alloy_primitives::{Address, Bytes, U256};
use zksync_types::{StorageKey, H256};

use crate::{convert::ConvertH160, is_system_address};

pub enum CallType {
    Call,
    Create(H256),
}

/// Interface for recording storage accesses via CALLs or CREATEs.
pub trait StorageAccessRecorder {
    fn start_recording(&mut self);
    fn stop_recording(&mut self);
    fn record_read(&mut self, key: &StorageKey, value: H256);
    fn record_write(&mut self, key: &StorageKey, old_value: H256, new_value: H256);
    fn record_call_start(
        &mut self,
        call_type: CallType,
        accessor: Address,
        account: Address,
        balance: U256,
        data: Vec<u8>,
        value: U256,
    );
    fn record_call_end(&mut self, accessor: Address, account: Address, new_balance: U256);
}

/// Represents the storage access during vm execution.
#[derive(Debug)]
pub struct StorageAccess {
    /// The account whose storage was accessed.
    pub account: Address,
    /// The slot that was accessed.
    pub slot: H256,
    /// If the access was a write.
    pub is_write: bool,
    /// The previous value of the slot.
    pub previous_value: H256,
    /// The new value of the slot.
    pub new_value: H256,
}

/// Account Access type
#[derive(Debug)]
pub enum AccountAccessKind {
    /// Access was a call.
    Call,
    /// Access was a create.
    Create,
}

/// Represents the account access during vm execution.
#[derive(Debug)]
pub struct AccountAccess {
    /// Call depth.
    pub depth: u64,
    /// Call type.
    pub kind: AccountAccessKind,
    /// Account that was accessed.
    pub account: Address,
    /// Accessor account.
    pub accessor: Address,
    /// Call data.
    pub data: Bytes,
    /// Deployed bytecode hash if CREATE.
    pub deployed_bytecode_hash: H256,
    /// Call value.
    pub value: U256,
    /// Previous balance of the accessed account.
    pub old_balance: U256,
    /// New balance of the accessed account.
    pub new_balance: U256,
    /// Storage slots that were accessed.
    pub storage_accesses: Vec<StorageAccess>,
}

#[derive(Debug, Default, Clone)]
pub struct CallAddresses {
    pub account: Address,
    pub accessor: Address,
}

#[derive(Debug, Default)]
pub struct AccountAccesses {
    records: Vec<AccountAccess>,
    pending: Vec<AccountAccess>,
    records_inner: Vec<AccountAccess>,
    is_recording: bool,
    /// Track the calls that must be skipped.
    /// We track this on a different stack to easily skip the `call_end`
    /// instances, if they were marked to be skipped in the `call_start`.
    call_skip_tracker: Vec<bool>,
    /// Mark the next call at a given depth and having the given address accesses.
    /// This is useful, for example to skip nested constructor calls after CREATE,
    /// to allow us to omit/flatten them like in EVM.
    skip_next_call: Option<(u64, CallAddresses)>,
}

impl AccountAccesses {
    pub fn get_records(self) -> Vec<AccountAccess> {
        assert!(
            self.call_skip_tracker.is_empty(),
            "call skip tracker is not empty; found calls without matching returns: {:?}",
            self.call_skip_tracker
        );
        assert!(
            self.skip_next_call.is_none(),
            "skip next call is not empty: {:?}",
            self.skip_next_call
        );
        assert!(
            self.pending.is_empty(),
            "pending call stack is not empty; found calls without matching returns: {:?}",
            self.pending
        );
        assert!(
            self.records_inner.is_empty(),
            "inner stack is not empty; found calls without matching returns: {:?}",
            self.records_inner
        );
        self.records
    }

    pub fn start_recording(&mut self) {
        self.is_recording = true;
    }

    pub fn stop_recording(&mut self) {
        self.is_recording = false;
    }

    pub fn record_read(&mut self, key: &StorageKey, value: H256) {
        if !self.is_recording {
            return;
        }

        // do not record system addresses
        if is_system_address(key.address().to_address()) {
            return;
        }

        let record = self.pending.last_mut().expect("expected at least one record");
        record.storage_accesses.push(StorageAccess {
            account: key.address().to_address(),
            slot: *key.key(),
            is_write: false,
            previous_value: value,
            new_value: value,
        });
    }

    pub fn record_write(&mut self, key: &StorageKey, old_value: H256, new_value: H256) {
        if !self.is_recording {
            return;
        }

        // do not record system addresses
        if is_system_address(key.address().to_address()) {
            return;
        }

        let record = self.pending.last_mut().expect("expected at least one record");
        record.storage_accesses.push(StorageAccess {
            account: key.address().to_address(),
            slot: *key.key(),
            is_write: true,
            previous_value: old_value,
            new_value,
        });
    }

    pub fn record_call_start(
        &mut self,
        call_type: CallType,
        accessor: Address,
        account: Address,
        balance: U256,
        data: Vec<u8>,
        value: U256,
    ) {
        if !self.is_recording {
            return;
        }

        // do not record calls to/from system addresses
        if is_system_address(accessor) || is_system_address(account) {
            self.call_skip_tracker.push(true);
            return;
        }

        let (kind, deployed_bytecode_hash) = match call_type {
            CallType::Call => (AccountAccessKind::Call, Default::default()),
            CallType::Create(bytecode_hash) => (AccountAccessKind::Create, bytecode_hash),
        };

        let last_depth = if !self.pending.is_empty() {
            self.pending.last().map(|record| record.depth).expect("must have at least one record")
        } else {
            self.records.last().map(|record| record.depth).unwrap_or_default()
        };
        let new_depth = last_depth.checked_add(1).expect("overflow in recording call depth");

        // For create we expect another CALL if the constructor is invoked. We need to skip/flatten
        // this call so it is consistent with CREATE in the EVM.
        match kind {
            AccountAccessKind::Create => {
                // skip the next nested call to the created address from the caller.
                self.skip_next_call =
                    Some((new_depth.saturating_add(1), CallAddresses { account, accessor }));
            }
            AccountAccessKind::Call => {
                if let Some((depth, call_addr)) = self.skip_next_call.take() {
                    if depth == new_depth &&
                        call_addr.accessor == accessor &&
                        call_addr.account == account
                    {
                        self.call_skip_tracker.push(true);
                        return;
                    }
                }
            }
        }

        self.call_skip_tracker.push(false);
        self.pending.push(AccountAccess {
            depth: new_depth,
            kind,
            account,
            accessor,
            data: Bytes::from(data),
            deployed_bytecode_hash,
            value,
            old_balance: balance,
            new_balance: U256::ZERO,
            storage_accesses: Default::default(),
        });
    }

    pub fn record_call_end(&mut self, _account: Address, _accessor: Address, new_balance: U256) {
        if !self.is_recording {
            return;
        }

        let skip_call =
            self.call_skip_tracker.pop().expect("unexpected return while skipping call recording");
        if skip_call {
            return;
        }

        let mut record = self.pending.pop().expect("unexpected return while recording call");
        record.new_balance = new_balance;

        if let Some((depth, _)) = &self.skip_next_call {
            if record.depth < *depth {
                // reset call skip if not encountered (depth has been crossed)
                self.skip_next_call = None;
            }
        }

        if self.pending.is_empty() {
            // no more pending records, append everything recorded so far.
            self.records.push(record);

            // also append the inner records.
            if !self.records_inner.is_empty() {
                self.records.extend(std::mem::take(&mut self.records_inner));
            }
        } else {
            // we have pending records, so record to inner.
            self.records_inner.push(record);
        }
    }
}
