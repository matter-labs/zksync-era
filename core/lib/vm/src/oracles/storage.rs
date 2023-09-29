use std::collections::HashMap;

use crate::implement_rollback;
use crate::rollback::snapshotted::Snapshotted;
use crate::rollback::{
    history_recorder::{HashMapHistoryEvent, HistoryRecorder, StorageWrapper},
    reversable_log::ReversableLog,
};
use crate::rollback::{LayeredRollback, Rollback};

use zk_evm::abstractions::RefundedAmounts;
use zk_evm::zkevm_opcode_defs::system_params::INITIAL_STORAGE_WRITE_PUBDATA_BYTES;
use zk_evm::{
    abstractions::{RefundType, Storage as VmStorageOracle},
    aux_structures::{LogQuery, Timestamp},
};

use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::utils::storage_key_for_eth_balance;
use zksync_types::{
    AccountTreeId, Address, StorageKey, StorageLogQuery, StorageLogQueryType, BOOTLOADER_ADDRESS,
    U256,
};
use zksync_utils::u256_to_h256;

// While the storage does not support different shards, it was decided to write the
// code of the StorageOracle with the shard parameters in mind.
pub(crate) fn triplet_to_storage_key(_shard_id: u8, address: Address, key: U256) -> StorageKey {
    StorageKey::new(AccountTreeId::new(address), u256_to_h256(key))
}

pub(crate) fn storage_key_of_log(query: &LogQuery) -> StorageKey {
    triplet_to_storage_key(query.shard_id, query.address, query.key)
}

#[derive(Debug)]
pub struct StorageOracle<S: WriteStorage>(LayeredRollback<StorageOracleInternals<S>>);

impl<S: WriteStorage> StorageOracle<S> {
    pub fn new(storage: StoragePtr<S>) -> Self {
        StorageOracle(LayeredRollback::new(StorageOracleInternals {
            storage: HistoryRecorder::from_inner(StorageWrapper::new(storage)),
            storage_logs: Default::default(),
            paid_changes: Default::default(),
            bytes_paid: Default::default(),
        }))
    }

    pub(crate) fn bytes_paid(&self) -> u32 {
        self.0.inner.bytes_paid.value
    }

    pub(crate) fn get_ptr(&self) -> std::rc::Rc<std::cell::RefCell<S>> {
        self.0.inner.storage.get_ptr()
    }

    #[cfg(test)]
    pub(crate) fn storage_logs(&self) -> ReversableLog<Box<StorageLogQuery>> {
        self.0.inner.storage_logs.clone()
    }

    #[cfg(test)]
    pub(crate) fn read_from_storage(&self, key: &StorageKey) -> U256 {
        self.0.inner.storage.read_from_storage(key)
    }

    /// Returns storage log queries where `log.log_query.timestamp >= from_timestamp`.
    pub(crate) fn storage_log_queries_after_timestamp(
        &self,
        from_timestamp: Timestamp,
    ) -> &[Box<StorageLogQuery>] {
        let logs = self.0.inner.storage_logs.events();

        // Select all of the last elements where l.log_query.timestamp >= from_timestamp.
        // Note, that using binary search here is dangerous, because the logs are not sorted by timestamp.
        logs.rsplit(|l| l.log_query.timestamp < from_timestamp)
            .next()
            .unwrap_or(&[])
    }

    pub(crate) fn get_final_log_queries(&self) -> Vec<StorageLogQuery> {
        self.0
            .inner
            .storage_logs
            .events()
            .iter()
            .map(|x| **x)
            .collect()
    }
}

impl<S: WriteStorage> Rollback for StorageOracle<S> {
    fn snapshot(&mut self) {
        self.0.snapshot()
    }
    fn rollback(&mut self) {
        self.0.rollback()
    }
    fn forget_snapshot(&mut self) {
        self.0.forget_snapshot()
    }
}

#[derive(Debug)]
struct StorageOracleInternals<S: WriteStorage> {
    // Access to the persistent storage. Please note that it
    // is used only for read access. All the actual writes happen
    // after the execution ended.
    pub(crate) storage: HistoryRecorder<StorageWrapper<S>>,

    pub(crate) storage_logs: ReversableLog<Box<StorageLogQuery>>,

    // The changes that have been paid for in previous transactions.
    // It is a mapping from storage key to the number of *bytes* that was paid by the user
    // to cover this slot.
    pub(crate) paid_changes: HistoryRecorder<HashMap<StorageKey, u32>>,
    pub(crate) bytes_paid: Snapshotted<u32>,
}

impl<S: WriteStorage> Rollback for StorageOracleInternals<S> {
    implement_rollback! {storage, storage_logs, paid_changes, bytes_paid}
}

impl<S: WriteStorage> StorageOracleInternals<S> {
    fn is_storage_key_free(&self, key: &StorageKey) -> bool {
        key.address() == &zksync_config::constants::SYSTEM_CONTEXT_ADDRESS
            || *key == storage_key_for_eth_balance(&BOOTLOADER_ADDRESS)
    }

    fn read_value(&mut self, mut query: LogQuery) -> LogQuery {
        let key = triplet_to_storage_key(query.shard_id, query.address, query.key);
        let current_value = self.storage.read_from_storage(&key);

        query.read_value = current_value;

        self.storage_logs.record(Box::new(StorageLogQuery {
            log_query: query,
            log_type: StorageLogQueryType::Read,
        }));

        query
    }

    fn write_value(&mut self, mut query: LogQuery) -> LogQuery {
        let key = triplet_to_storage_key(query.shard_id, query.address, query.key);
        let current_value = self.storage.write_to_storage(key, query.written_value);

        let is_initial_write = self.storage.get_ptr().borrow_mut().is_write_initial(&key);
        let log_query_type = if is_initial_write {
            StorageLogQueryType::InitialWrite
        } else {
            StorageLogQueryType::RepeatedWrite
        };

        query.read_value = current_value;

        self.storage_logs.record(Box::new(StorageLogQuery {
            log_query: query,
            log_type: log_query_type,
        }));

        query
    }

    // Returns the amount of funds that has been already paid for writes into the storage slot
    fn prepaid_for_write(&self, storage_key: &StorageKey) -> u32 {
        self.paid_changes
            .inner()
            .get(storage_key)
            .copied()
            .unwrap_or_default()
    }

    fn base_price_for_write(&self, query: &LogQuery) -> u32 {
        let storage_key = storage_key_of_log(query);

        if self.is_storage_key_free(&storage_key) {
            return 0;
        }

        let is_initial_write = self
            .storage
            .get_ptr()
            .borrow_mut()
            .is_write_initial(&storage_key);

        get_pubdata_price_bytes(query, is_initial_write)
    }

    // Returns the price of the update in terms of pubdata bytes.
    // TODO (SMA-1701): update VM to accept gas instead of pubdata.
    fn value_update_price(&self, query: &LogQuery) -> u32 {
        let storage_key = storage_key_of_log(query);

        let base_cost = self.base_price_for_write(query);

        let already_paid = self.prepaid_for_write(&storage_key);

        if base_cost <= already_paid {
            // Some other transaction has already paid for this slot, no need to pay anything
            0u32
        } else {
            base_cost - already_paid
        }
    }
}

impl<S: WriteStorage> VmStorageOracle for StorageOracle<S> {
    // Perform a storage read/write access by taking an partially filled query
    // and returning filled query and cold/warm marker for pricing purposes
    fn execute_partial_query(
        &mut self,
        _monotonic_cycle_counter: u32,
        query: LogQuery,
    ) -> LogQuery {
        // tracing::trace!(
        //     "execute partial query cyc {:?} addr {:?} key {:?}, rw {:?}, wr {:?}, tx {:?}",
        //     _monotonic_cycle_counter,
        //     query.address,
        //     query.key,
        //     query.rw_flag,
        //     query.written_value,
        //     query.tx_number_in_block
        // );
        assert!(!query.rollback);
        if query.rw_flag {
            // The number of bytes that have been compensated by the user to perform this write
            let storage_key = storage_key_of_log(&query);

            // It is considered that the user has paid for the whole base price for the writes
            let to_pay_by_user = self.0.inner.base_price_for_write(&query);
            let prepaid = self.0.inner.prepaid_for_write(&storage_key);

            if to_pay_by_user > prepaid {
                self.0
                    .inner
                    .paid_changes
                    .apply_historic_record(HashMapHistoryEvent {
                        key: storage_key,
                        value: Some(to_pay_by_user),
                    });
                self.0.inner.bytes_paid.value += to_pay_by_user - prepaid;
            }
            self.0.inner.write_value(query)
        } else {
            self.0.inner.read_value(query)
        }
    }

    // We can return the size of the refund before each storage query.
    // Note, that while the `RefundType` allows to provide refunds both in
    // `ergs` and `pubdata`, only refunds in pubdata will be compensated for the users
    fn estimate_refunds_for_write(
        &mut self, // to avoid any hacks inside, like prefetch
        _monotonic_cycle_counter: u32,
        partial_query: &LogQuery,
    ) -> RefundType {
        let price_to_pay = self.0.inner.value_update_price(partial_query);

        RefundType::RepeatedWrite(RefundedAmounts {
            ergs: 0,
            // `INITIAL_STORAGE_WRITE_PUBDATA_BYTES` is the default amount of pubdata bytes the user pays for.
            pubdata_bytes: (INITIAL_STORAGE_WRITE_PUBDATA_BYTES as u32) - price_to_pay,
        })
    }

    // Indicate a start of execution frame for rollback purposes
    fn start_frame(&mut self, _: Timestamp) {
        self.0.internal_snapshot();
    }

    // Indicate that execution frame went out from the scope, so we can
    // log the history and either rollback immediately or keep records to rollback later
    fn finish_frame(&mut self, _: Timestamp, panicked: bool) {
        if panicked {
            self.0.internal_rollback();
        } else {
            self.0.internal_forget_snapshot();
        }
    }
}

/// Returns the number of bytes needed to publish a slot.
// Since we need to publish the state diffs onchain, for each of the updated storage slot
// we basically need to publish the following pair: (<storage_key, new_value>).
// While new_value is always 32 bytes long, for key we use the following optimization:
//   - The first time we publish it, we use 32 bytes.
//         Then, we remember a 8-byte id for this slot and assign it to it. We call this initial write.
//   - The second time we publish it, we will use this 8-byte instead of the 32 bytes of the entire key.
//         So the total size of the publish pubdata is 40 bytes. We call this kind of write the repeated one
fn get_pubdata_price_bytes(_query: &LogQuery, is_initial: bool) -> u32 {
    // TODO (SMA-1702): take into account the content of the log query, i.e. values that contain mostly zeroes
    // should cost less.
    if is_initial {
        zk_evm::zkevm_opcode_defs::system_params::INITIAL_STORAGE_WRITE_PUBDATA_BYTES as u32
    } else {
        zk_evm::zkevm_opcode_defs::system_params::REPEATED_STORAGE_WRITE_PUBDATA_BYTES as u32
    }
}
