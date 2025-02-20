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
    fn pop_call_end_addresses(&mut self) -> CallAddresses;
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
    /// Track `FarCalls`s to allow matching them with their respective `Ret` opcodes.
    /// zkEVM erases the `msg.sender` and `code_address` for certain calls like to MsgSimulator,
    /// so we track them using this strategy to know when to skip the respective `Ret`s of already
    /// skipped `FarCalls`s.
    call_tracker: Vec<CallAddresses>,
    last_create_addresses: Vec<Address>,
}

impl AccountAccesses {
    pub fn get_records(self) -> Vec<AccountAccess> {
        assert!(
            self.last_create_addresses.is_empty(),
            "last create address is not empty; expected a CALL after CREATE to pop it: {:?}",
            self.last_create_addresses
        );
        assert!(
            self.call_tracker.is_empty(),
            "CallTracker stack is not empty; found calls without matching returns: {:?}",
            self.call_tracker
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

        self.call_tracker.push(CallAddresses { account, accessor });

        // do not record calls to/from system addresses
        if is_system_address(accessor) || is_system_address(account) {
            return;
        }

        let (kind, deployed_bytecode_hash) = match call_type {
            CallType::Call => (AccountAccessKind::Call, Default::default()),
            CallType::Create(bytecode_hash) => (AccountAccessKind::Create, bytecode_hash),
        };

        // For create we expect another CALL with empty data to the newly created address that we
        // should skip recording, so we track the address until a matching CALL pops it.
        if matches!(kind, AccountAccessKind::Create) {
            self.last_create_addresses.push(account)
        }

        let last_depth = if !self.pending.is_empty() {
            self.pending.last().map(|record| record.depth).expect("must have at least one record")
        } else {
            self.records.last().map(|record| record.depth).unwrap_or_default()
        };
        let new_depth = last_depth.checked_add(1).expect("overflow in recording call depth");

        self.pending.push(AccountAccess {
            depth: new_depth,
            kind,
            account,
            accessor,
            data: Bytes::from(data),
            deployed_bytecode_hash,
            value,
            old_balance: balance,
            new_balance: U256::ZERO, //balance.saturating_add(value),
            storage_accesses: Default::default(),
        });
    }

    pub fn record_call_end(&mut self, account: Address, accessor: Address, new_balance: U256) {
        if !self.is_recording {
            return;
        }

        // do not record calls to/from system addresses
        if is_system_address(accessor) || is_system_address(account) {
            return;
        }

        let mut record = self.pending.pop().expect("unexpected return while recording call");
        record.new_balance = new_balance;

        // For create we expect another CALL with empty data to the newly created address that we
        // should skip recording
        if let Some(last_create_addr) = self.last_create_addresses.last().cloned() {
            if matches!(record.kind, AccountAccessKind::Call) &&
                record.account == last_create_addr &&
                record.data.is_empty()
            {
                // skip recording this call
                self.last_create_addresses.pop();
                return;
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

    pub fn pop_call_end_addresses(&mut self) -> CallAddresses {
        self.call_tracker.pop().expect("unexpected request for call addresses; none on stack")
    }
}
