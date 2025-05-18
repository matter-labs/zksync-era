use std::collections::HashMap;

use zk_evm_1_4_0::{
    abstractions::{RefundType, RefundedAmounts, Storage as VmStorageOracle},
    aux_structures::{LogQuery, Timestamp},
    zkevm_opcode_defs::system_params::INITIAL_STORAGE_WRITE_PUBDATA_BYTES,
};
use zksync_types::{
    u256_to_h256,
    utils::storage_key_for_eth_balance,
    writes::{
        compression::compress_with_best_strategy, BYTES_PER_DERIVED_KEY,
        BYTES_PER_ENUMERATION_INDEX,
    },
    AccountTreeId, Address, StorageKey, StorageLogKind, BOOTLOADER_ADDRESS, U256,
};

use crate::{
    interface::storage::{StoragePtr, WriteStorage},
    vm_boojum_integration::{
        old_vm::{
            history_recorder::{
                AppDataFrameManagerWithHistory, HashMapHistoryEvent, HistoryEnabled, HistoryMode,
                HistoryRecorder, StorageWrapper, VectorHistoryEvent, WithHistory,
            },
            oracles::OracleWithHistory,
        },
        utils::logs::StorageLogQuery,
    },
};

// While the storage does not support different shards, it was decided to write the
// code of the StorageOracle with the shard parameters in mind.
pub(crate) fn triplet_to_storage_key(_shard_id: u8, address: Address, key: U256) -> StorageKey {
    StorageKey::new(AccountTreeId::new(address), u256_to_h256(key))
}

pub(crate) fn storage_key_of_log(query: &LogQuery) -> StorageKey {
    triplet_to_storage_key(query.shard_id, query.address, query.key)
}

#[derive(Debug)]
pub struct StorageOracle<S: WriteStorage, H: HistoryMode> {
    // Access to the persistent storage. Please note that it
    // is used only for read access. All the actual writes happen
    // after the execution ended.
    pub(crate) storage: HistoryRecorder<StorageWrapper<S>, H>,

    pub(crate) frames_stack: AppDataFrameManagerWithHistory<Box<StorageLogQuery>, H>,

    // The changes that have been paid for in previous transactions.
    // It is a mapping from storage key to the number of *bytes* that was paid by the user
    // to cover this slot.
    pub(crate) pre_paid_changes: HistoryRecorder<HashMap<StorageKey, u32>, H>,

    // The changes that have been paid for in the current transaction
    pub(crate) paid_changes: HistoryRecorder<HashMap<StorageKey, u32>, H>,

    // The map that contains all the first values read from storage for each slot.
    // While formally it does not have to be rolled back, we still do it to avoid memory bloat
    // for unused slots.
    pub(crate) initial_values: HistoryRecorder<HashMap<StorageKey, U256>, H>,

    // Storage refunds that oracle has returned in `estimate_refunds_for_write`.
    pub(crate) returned_refunds: HistoryRecorder<Vec<u32>, H>,

    // Keeps track of storage keys that were ever written to.
    pub(crate) written_keys: HistoryRecorder<HashMap<StorageKey, ()>, HistoryEnabled>,
    // Keeps track of storage keys that were ever read.
    pub(crate) read_keys: HistoryRecorder<HashMap<StorageKey, ()>, HistoryEnabled>,
}

impl<S: WriteStorage> OracleWithHistory for StorageOracle<S, HistoryEnabled> {
    fn rollback_to_timestamp(&mut self, timestamp: Timestamp) {
        self.storage.rollback_to_timestamp(timestamp);
        self.frames_stack.rollback_to_timestamp(timestamp);
        self.pre_paid_changes.rollback_to_timestamp(timestamp);
        self.paid_changes.rollback_to_timestamp(timestamp);
        self.initial_values.rollback_to_timestamp(timestamp);
        self.returned_refunds.rollback_to_timestamp(timestamp);
        self.written_keys.rollback_to_timestamp(timestamp);
        self.read_keys.rollback_to_timestamp(timestamp);
    }
}

impl<S: WriteStorage, H: HistoryMode> StorageOracle<S, H> {
    pub fn new(storage: StoragePtr<S>) -> Self {
        Self {
            storage: HistoryRecorder::from_inner(StorageWrapper::new(storage)),
            frames_stack: Default::default(),
            pre_paid_changes: Default::default(),
            paid_changes: Default::default(),
            initial_values: Default::default(),
            returned_refunds: Default::default(),
            written_keys: Default::default(),
            read_keys: Default::default(),
        }
    }

    pub fn delete_history(&mut self) {
        self.storage.delete_history();
        self.frames_stack.delete_history();
        self.pre_paid_changes.delete_history();
        self.paid_changes.delete_history();
        self.initial_values.delete_history();
        self.returned_refunds.delete_history();
        self.written_keys.delete_history();
        self.read_keys.delete_history();
    }

    fn is_storage_key_free(&self, key: &StorageKey) -> bool {
        key.address() == &zksync_system_constants::SYSTEM_CONTEXT_ADDRESS
            || *key == storage_key_for_eth_balance(&BOOTLOADER_ADDRESS)
    }

    fn get_initial_value(&self, storage_key: &StorageKey) -> Option<U256> {
        self.initial_values.inner().get(storage_key).copied()
    }

    fn set_initial_value(&mut self, storage_key: &StorageKey, value: U256, timestamp: Timestamp) {
        if !self.initial_values.inner().contains_key(storage_key) {
            self.initial_values.insert(*storage_key, value, timestamp);
        }
    }

    fn read_value(&mut self, mut query: LogQuery) -> LogQuery {
        let key = triplet_to_storage_key(query.shard_id, query.address, query.key);

        if !self.read_keys.inner().contains_key(&key) {
            self.read_keys.insert(key, (), query.timestamp);
        }
        let current_value = self.storage.read_from_storage(&key);

        query.read_value = current_value;

        self.set_initial_value(&key, current_value, query.timestamp);

        self.frames_stack.push_forward(
            Box::new(StorageLogQuery {
                log_query: query,
                log_type: StorageLogKind::Read,
            }),
            query.timestamp,
        );

        query
    }

    fn write_value(&mut self, query: LogQuery) -> LogQuery {
        let key = triplet_to_storage_key(query.shard_id, query.address, query.key);
        if !self.written_keys.inner().contains_key(&key) {
            self.written_keys.insert(key, (), query.timestamp);
        }
        let current_value =
            self.storage
                .write_to_storage(key, query.written_value, query.timestamp);

        let is_initial_write = self.storage.get_ptr().borrow_mut().is_write_initial(&key);
        let log_query_type = if is_initial_write {
            StorageLogKind::InitialWrite
        } else {
            StorageLogKind::RepeatedWrite
        };

        self.set_initial_value(&key, current_value, query.timestamp);

        let mut storage_log_query = StorageLogQuery {
            log_query: query,
            log_type: log_query_type,
        };
        self.frames_stack
            .push_forward(Box::new(storage_log_query), query.timestamp);
        storage_log_query.log_query.rollback = true;
        self.frames_stack
            .push_rollback(Box::new(storage_log_query), query.timestamp);
        storage_log_query.log_query.rollback = false;

        query
    }

    // Returns the amount of funds that has been already paid for writes into the storage slot
    fn prepaid_for_write(&self, storage_key: &StorageKey) -> u32 {
        self.paid_changes
            .inner()
            .get(storage_key)
            .copied()
            .unwrap_or_else(|| {
                self.pre_paid_changes
                    .inner()
                    .get(storage_key)
                    .copied()
                    .unwrap_or(0)
            })
    }

    // Remembers the changes that have been paid for in the current transaction.
    // It also returns how much pubdata did the user pay for and how much was actually published.
    pub(crate) fn save_paid_changes(&mut self, timestamp: Timestamp) -> u32 {
        let mut published = 0;

        let modified_keys = self
            .paid_changes
            .inner()
            .iter()
            .map(|(k, v)| (*k, *v))
            .collect::<Vec<_>>();

        for (key, _) in modified_keys {
            // It is expected that for each slot for which we have paid changes, there is some
            // first slot value already read.
            let first_slot_value = self.initial_values.inner().get(&key).copied().unwrap();

            // This is the value has been written to the storage slot at the end.
            let current_slot_value = self.storage.read_from_storage(&key);

            let required_pubdata =
                self.base_price_for_write(&key, first_slot_value, current_slot_value);

            // We assume that `prepaid_for_slot` represents both the number of pubdata published and the number of bytes paid by the previous transactions
            // as they should be identical.
            let prepaid_for_slot = self
                .pre_paid_changes
                .inner()
                .get(&key)
                .copied()
                .unwrap_or_default();

            published += required_pubdata.saturating_sub(prepaid_for_slot);

            // We remove the slot from the paid changes and move to the pre-paid changes as
            // the transaction ends.
            self.paid_changes.remove(key, timestamp);
            self.pre_paid_changes
                .insert(key, prepaid_for_slot.max(required_pubdata), timestamp);
        }

        published
    }

    fn base_price_for_write_query(&self, query: &LogQuery) -> u32 {
        let storage_key = storage_key_of_log(query);

        let initial_value = self
            .get_initial_value(&storage_key)
            .unwrap_or(self.storage.read_from_storage(&storage_key));

        self.base_price_for_write(&storage_key, initial_value, query.written_value)
    }

    pub(crate) fn base_price_for_write(
        &self,
        storage_key: &StorageKey,
        prev_value: U256,
        new_value: U256,
    ) -> u32 {
        if self.is_storage_key_free(storage_key) || prev_value == new_value {
            return 0;
        }

        let is_initial_write = self
            .storage
            .get_ptr()
            .borrow_mut()
            .is_write_initial(storage_key);

        get_pubdata_price_bytes(prev_value, new_value, is_initial_write)
    }

    // Returns the price of the update in terms of pubdata bytes.
    // TODO (SMA-1701): update VM to accept gas instead of pubdata.
    fn value_update_price(&mut self, query: &LogQuery) -> u32 {
        let storage_key = storage_key_of_log(query);

        let base_cost = self.base_price_for_write_query(query);

        let initial_value = self
            .get_initial_value(&storage_key)
            .unwrap_or(self.storage.read_from_storage(&storage_key));

        self.set_initial_value(&storage_key, initial_value, query.timestamp);

        let already_paid = self.prepaid_for_write(&storage_key);

        // No need to pay anything if some other transaction has already paid for this slot
        base_cost.saturating_sub(already_paid)
    }

    /// Returns storage log queries from current frame where `log.log_query.timestamp >= from_timestamp`.
    pub(crate) fn storage_log_queries_after_timestamp(
        &self,
        from_timestamp: Timestamp,
    ) -> &[Box<StorageLogQuery>] {
        let logs = self.frames_stack.forward().current_frame();

        // Select all of the last elements where `l.log_query.timestamp >= from_timestamp`.
        // Note, that using binary search here is dangerous, because the logs are not sorted by timestamp.
        logs.rsplit(|l| l.log_query.timestamp < from_timestamp)
            .next()
            .unwrap_or(&[])
    }

    pub(crate) fn get_final_log_queries(&self) -> Vec<StorageLogQuery> {
        assert_eq!(
            self.frames_stack.len(),
            1,
            "VM finished execution in unexpected state"
        );

        self.frames_stack
            .forward()
            .current_frame()
            .iter()
            .map(|x| **x)
            .collect()
    }

    pub(crate) fn get_size(&self) -> usize {
        let frames_stack_size = self.frames_stack.get_size();
        let paid_changes_size =
            self.paid_changes.inner().len() * std::mem::size_of::<(StorageKey, u32)>();

        frames_stack_size + paid_changes_size
    }

    pub(crate) fn get_history_size(&self) -> usize {
        let storage_size = self.storage.borrow_history(|h| h.len(), 0)
            * std::mem::size_of::<<StorageWrapper<S> as WithHistory>::HistoryRecord>();
        let frames_stack_size = self.frames_stack.get_history_size();
        let paid_changes_size = self.paid_changes.borrow_history(|h| h.len(), 0)
            * std::mem::size_of::<<HashMap<StorageKey, u32> as WithHistory>::HistoryRecord>();
        storage_size + frames_stack_size + paid_changes_size
    }
}

impl<S: WriteStorage, H: HistoryMode> VmStorageOracle for StorageOracle<S, H> {
    // Perform a storage read/write access by taking an partially filled query
    // and returning filled query and cold/warm marker for pricing purposes
    fn execute_partial_query(
        &mut self,
        _monotonic_cycle_counter: u32,
        mut query: LogQuery,
    ) -> LogQuery {
        //```
        // tracing::trace!(
        //     "execute partial query cyc {:?} addr {:?} key {:?}, rw {:?}, wr {:?}, tx {:?}",
        //     _monotonic_cycle_counter,
        //     query.address,
        //     query.key,
        //     query.rw_flag,
        //     query.written_value,
        //     query.tx_number_in_block
        // );
        //```
        assert!(!query.rollback);
        if query.rw_flag {
            // The number of bytes that have been compensated by the user to perform this write
            let storage_key = storage_key_of_log(&query);
            let read_value = self.storage.read_from_storage(&storage_key);
            query.read_value = read_value;

            // It is considered that the user has paid for the whole base price for the writes
            let to_pay_by_user = self.base_price_for_write_query(&query);
            let prepaid = self.prepaid_for_write(&storage_key);

            if to_pay_by_user > prepaid {
                self.paid_changes.apply_historic_record(
                    HashMapHistoryEvent {
                        key: storage_key,
                        value: Some(to_pay_by_user),
                    },
                    query.timestamp,
                );
            }
            self.write_value(query)
        } else {
            self.read_value(query)
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
        let storage_key = storage_key_of_log(partial_query);
        let mut partial_query = *partial_query;
        let read_value = self.storage.read_from_storage(&storage_key);
        partial_query.read_value = read_value;

        let price_to_pay = self
            .value_update_price(&partial_query)
            .min(INITIAL_STORAGE_WRITE_PUBDATA_BYTES as u32);

        let refund = RefundType::RepeatedWrite(RefundedAmounts {
            ergs: 0,
            // `INITIAL_STORAGE_WRITE_PUBDATA_BYTES` is the default amount of pubdata bytes the user pays for.
            pubdata_bytes: (INITIAL_STORAGE_WRITE_PUBDATA_BYTES as u32) - price_to_pay,
        });
        self.returned_refunds.apply_historic_record(
            VectorHistoryEvent::Push(refund.pubdata_refund()),
            partial_query.timestamp,
        );

        refund
    }

    // Indicate a start of execution frame for rollback purposes
    fn start_frame(&mut self, timestamp: Timestamp) {
        self.frames_stack.push_frame(timestamp);
    }

    // Indicate that execution frame went out from the scope, so we can
    // log the history and either rollback immediately or keep records to rollback later
    fn finish_frame(&mut self, timestamp: Timestamp, panicked: bool) {
        // If we panic then we append forward and rollbacks to the forward of parent,
        // otherwise we place rollbacks of child before rollbacks of the parent
        if panicked {
            // perform actual rollback
            for query in self.frames_stack.rollback().current_frame().iter().rev() {
                let read_value = match query.log_type {
                    StorageLogKind::Read => {
                        // Having Read logs in rollback is not possible
                        tracing::warn!("Read log in rollback queue {:?}", query);
                        continue;
                    }
                    StorageLogKind::InitialWrite | StorageLogKind::RepeatedWrite => {
                        query.log_query.read_value
                    }
                };

                let LogQuery { written_value, .. } = query.log_query;
                let key = triplet_to_storage_key(
                    query.log_query.shard_id,
                    query.log_query.address,
                    query.log_query.key,
                );
                let current_value = self.storage.write_to_storage(
                    key,
                    // NOTE, that since it is a rollback query,
                    // the `read_value` is being set
                    read_value, timestamp,
                );

                // Additional validation that the current value was correct
                // Unwrap is safe because the return value from `write_inner` is the previous value in this leaf.
                // It is impossible to set leaf value to `None`
                assert_eq!(current_value, written_value);
            }

            self.frames_stack
                .move_rollback_to_forward(|_| true, timestamp);
        }
        self.frames_stack.merge_frame(timestamp);
    }
}

/// Returns the number of bytes needed to publish a slot.
// Since we need to publish the state diffs onchain, for each of the updated storage slot
// we basically need to publish the following pair: `(<storage_key, compressed_new_value>)`.
// For key we use the following optimization:
//   - The first time we publish it, we use 32 bytes.
//         Then, we remember a 8-byte id for this slot and assign it to it. We call this initial write.
//   - The second time we publish it, we will use the 4/5 byte representation of this 8-byte instead of the 32
//     bytes of the entire key.
// For value compression, we use a metadata byte which holds the length of the value and the operation from the
// previous state to the new state, and the compressed value. The maximum for this is 33 bytes.
// Total bytes for initial writes then becomes 65 bytes and repeated writes becomes 38 bytes.
fn get_pubdata_price_bytes(initial_value: U256, final_value: U256, is_initial: bool) -> u32 {
    // TODO (SMA-1702): take into account the content of the log query, i.e. values that contain mostly zeroes
    // should cost less.

    let compressed_value_size =
        compress_with_best_strategy(initial_value, final_value).len() as u32;

    if is_initial {
        (BYTES_PER_DERIVED_KEY as u32) + compressed_value_size
    } else {
        (BYTES_PER_ENUMERATION_INDEX as u32) + compressed_value_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_pubdata_price_bytes() {
        let initial_value = U256::default();
        let final_value = U256::from(92122);
        let is_initial = true;

        let compression_len = 4;

        let initial_bytes_price = get_pubdata_price_bytes(initial_value, final_value, is_initial);
        let repeated_bytes_price = get_pubdata_price_bytes(initial_value, final_value, !is_initial);

        assert_eq!(
            initial_bytes_price,
            (compression_len + BYTES_PER_DERIVED_KEY as usize) as u32
        );
        assert_eq!(
            repeated_bytes_price,
            (compression_len + BYTES_PER_ENUMERATION_INDEX as usize) as u32
        );
    }
}
