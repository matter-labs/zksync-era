use std::collections::HashMap;

use zk_evm_1_5_2::{
    abstractions::{Storage as VmStorageOracle, StorageAccessRefund},
    aux_structures::{LogQuery, PubdataCost, Timestamp},
    zkevm_opcode_defs::system_params::{
        STORAGE_ACCESS_COLD_READ_COST, STORAGE_ACCESS_COLD_WRITE_COST,
        STORAGE_ACCESS_WARM_READ_COST, STORAGE_ACCESS_WARM_WRITE_COST, STORAGE_AUX_BYTE,
        TRANSIENT_STORAGE_AUX_BYTE,
    },
};
use zksync_types::{
    h256_to_u256, u256_to_h256,
    utils::storage_key_for_eth_balance,
    writes::{
        compression::compress_with_best_strategy, BYTES_PER_DERIVED_KEY,
        BYTES_PER_ENUMERATION_INDEX,
    },
    AccountTreeId, Address, StorageKey, StorageLogKind, BOOTLOADER_ADDRESS, U256,
};

use crate::{
    glue::GlueInto,
    interface::storage::{StoragePtr, WriteStorage},
    vm_latest::{
        old_vm::{
            history_recorder::{
                AppDataFrameManagerWithHistory, HashMapHistoryEvent, HistoryEnabled, HistoryMode,
                HistoryRecorder, StorageWrapper, TransientStorageWrapper, VectorHistoryEvent,
                WithHistory,
            },
            oracles::OracleWithHistory,
        },
        utils::logs::StorageLogQuery,
    },
};

// We employ the following rules for cold/warm storage rules:
// - We price a single "I/O" access as 2k ergs. This means that reading a single storage slot
//   would cost 2k ergs, while writing to it would 4k ergs (since it involves both reading during execution and writing at the end of it).
// - Thereafter, "warm" reads cost 30 ergs, while "warm" writes cost 60 ergs. Warm writes to account cost more for the fact that they may be reverted
//   and so require more RAM to store them.

const WARM_READ_REFUND: u32 = STORAGE_ACCESS_COLD_READ_COST - STORAGE_ACCESS_WARM_READ_COST;
const WARM_WRITE_REFUND: u32 = STORAGE_ACCESS_COLD_WRITE_COST - STORAGE_ACCESS_WARM_WRITE_COST;
const COLD_WRITE_AFTER_WARM_READ_REFUND: u32 = STORAGE_ACCESS_COLD_READ_COST;

// While the storage does not support different shards, it was decided to write the
// code of the StorageOracle with the shard parameters in mind.
pub(crate) fn triplet_to_storage_key(_shard_id: u8, address: Address, key: U256) -> StorageKey {
    StorageKey::new(AccountTreeId::new(address), u256_to_h256(key))
}

pub(crate) fn storage_key_of_log(query: &LogQuery) -> StorageKey {
    triplet_to_storage_key(query.shard_id, query.address, query.key)
}

/// The same struct as `zk_evm_1_5_2::aux_structures::LogQuery`, but without the fields that
/// are not needed to maintain the frame stack of the transient storage.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ReducedTstoreLogQuery {
    shard_id: u8,
    address: Address,
    key: U256,
    written_value: U256,
    read_value: U256,
    rw_flag: bool,
    timestamp: Timestamp,
    rollback: bool,
}

impl From<LogQuery> for ReducedTstoreLogQuery {
    fn from(query: LogQuery) -> Self {
        Self {
            shard_id: query.shard_id,
            address: query.address,
            key: query.key,
            written_value: query.written_value,
            read_value: query.read_value,
            rw_flag: query.rw_flag,
            timestamp: query.timestamp,
            rollback: query.rollback,
        }
    }
}

#[derive(Debug)]
pub struct StorageOracle<S: WriteStorage, H: HistoryMode> {
    // Access to the persistent storage. Please note that it
    // is used only for read access. All the actual writes happen
    // after the execution ended.
    pub(crate) storage: HistoryRecorder<StorageWrapper<S>, H>,

    // Wrapper over transient storage.
    pub(crate) transient_storage: HistoryRecorder<TransientStorageWrapper, H>,

    pub(crate) storage_frames_stack: AppDataFrameManagerWithHistory<Box<StorageLogQuery>, H>,

    pub(crate) transient_storage_frames_stack:
        AppDataFrameManagerWithHistory<Box<ReducedTstoreLogQuery>, H>,

    // The changes that have been paid for in the current transaction.
    // It is a mapping from storage key to the number of *bytes* that was paid by the user
    // to cover this slot.
    // Note, that this field has to be rolled back in case of a panicking frame.
    pub(crate) paid_changes: HistoryRecorder<HashMap<StorageKey, u32>, H>,

    // The map that contains all the first values read from storage for each slot.
    // While formally it does not have to be capable of rolling back, we still do it to avoid memory bloat
    // for unused slots.
    pub(crate) initial_values: HistoryRecorder<HashMap<StorageKey, U256>, H>,

    // Storage I/O refunds that oracle has returned in `get_access_refund`.
    pub(crate) returned_io_refunds: HistoryRecorder<Vec<u32>, H>,

    // The pubdata costs that oracle has returned in `execute_partial_query`.
    // Note, that these can be negative, since the user may rollback some storage changes.
    pub(crate) returned_pubdata_costs: HistoryRecorder<Vec<i32>, H>,

    // Keeps track of storage keys that were ever written to. This is needed for circuits tracer, this is why
    // we don't roll this value back in case of a panicked frame.
    pub(crate) written_storage_keys: HistoryRecorder<HashMap<StorageKey, ()>, HistoryEnabled>,
    // Keeps track of storage keys that were ever read. This is needed for circuits tracer, this is why
    // we don't roll this value back in case of a panicked frame.
    // Note, that it is a superset of `written_storage_keys`, since every written key was also read at some point.
    pub(crate) read_storage_keys: HistoryRecorder<HashMap<StorageKey, ()>, HistoryEnabled>,
}

impl<S: WriteStorage> OracleWithHistory for StorageOracle<S, HistoryEnabled> {
    fn rollback_to_timestamp(&mut self, timestamp: Timestamp) {
        self.storage.rollback_to_timestamp(timestamp);
        self.storage_frames_stack.rollback_to_timestamp(timestamp);
        self.transient_storage.rollback_to_timestamp(timestamp);
        self.transient_storage_frames_stack
            .rollback_to_timestamp(timestamp);
        self.paid_changes.rollback_to_timestamp(timestamp);
        self.initial_values.rollback_to_timestamp(timestamp);
        self.returned_io_refunds.rollback_to_timestamp(timestamp);
        self.returned_pubdata_costs.rollback_to_timestamp(timestamp);
        self.written_storage_keys.rollback_to_timestamp(timestamp);
        self.read_storage_keys.rollback_to_timestamp(timestamp);
    }
}

impl<S: WriteStorage, H: HistoryMode> StorageOracle<S, H> {
    pub fn new(storage: StoragePtr<S>) -> Self {
        Self {
            storage: HistoryRecorder::from_inner(StorageWrapper::new(storage)),
            transient_storage: Default::default(),
            storage_frames_stack: Default::default(),
            transient_storage_frames_stack: Default::default(),
            paid_changes: Default::default(),
            initial_values: Default::default(),
            returned_io_refunds: Default::default(),
            returned_pubdata_costs: Default::default(),
            written_storage_keys: Default::default(),
            read_storage_keys: Default::default(),
        }
    }

    pub fn delete_history(&mut self) {
        self.storage.delete_history();
        self.transient_storage.delete_history();
        self.storage_frames_stack.delete_history();
        self.paid_changes.delete_history();
        self.initial_values.delete_history();
        self.returned_io_refunds.delete_history();
        self.returned_pubdata_costs.delete_history();
        self.written_storage_keys.delete_history();
        self.read_storage_keys.delete_history();
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

    fn record_storage_read(&mut self, query: LogQuery) {
        let storage_log_query = StorageLogQuery {
            log_query: query,
            log_type: StorageLogKind::Read,
        };

        self.storage_frames_stack
            .push_forward(Box::new(storage_log_query), query.timestamp);
    }

    fn write_storage_value(&mut self, query: LogQuery) {
        let key = triplet_to_storage_key(query.shard_id, query.address, query.key);
        if !self.written_storage_keys.inner().contains_key(&key) {
            self.written_storage_keys.insert(key, (), query.timestamp);
        }

        self.storage
            .write_to_storage(key, query.written_value, query.timestamp);

        let is_initial_write = self.storage.get_ptr().borrow_mut().is_write_initial(&key);
        let log_query_type = if is_initial_write {
            StorageLogKind::InitialWrite
        } else {
            StorageLogKind::RepeatedWrite
        };

        let mut storage_log_query = StorageLogQuery {
            log_query: query,
            log_type: log_query_type,
        };
        self.storage_frames_stack
            .push_forward(Box::new(storage_log_query), query.timestamp);
        storage_log_query.log_query.rollback = true;
        self.storage_frames_stack
            .push_rollback(Box::new(storage_log_query), query.timestamp);
    }

    fn record_transient_storage_read(&mut self, query: ReducedTstoreLogQuery) {
        self.transient_storage_frames_stack
            .push_forward(Box::new(query), query.timestamp);
    }

    fn write_transient_storage_value(&mut self, mut query: ReducedTstoreLogQuery) {
        let key = triplet_to_storage_key(query.shard_id, query.address, query.key);

        self.transient_storage
            .write_to_storage(key, query.written_value, query.timestamp);

        self.transient_storage_frames_stack
            .push_forward(Box::new(query), query.timestamp);

        query.rollback = true;
        self.transient_storage_frames_stack
            .push_rollback(Box::new(query), query.timestamp);
    }

    // Returns the amount of funds that has been already paid for writes into the storage slot
    fn prepaid_for_write(&self, storage_key: &StorageKey) -> u32 {
        self.paid_changes
            .inner()
            .get(storage_key)
            .copied()
            .unwrap_or(0)
    }

    fn base_price_for_write_query(&self, query: &LogQuery) -> u32 {
        let storage_key = storage_key_of_log(query);

        self.base_price_for_key_write(&storage_key, query.written_value)
    }

    fn base_price_for_key_write(&self, storage_key: &StorageKey, new_value: U256) -> u32 {
        let initial_value = self
            .get_initial_value(storage_key)
            .unwrap_or(self.storage.read_from_storage(storage_key));

        self.base_price_for_write(storage_key, initial_value, new_value)
    }

    fn get_storage_access_refund(&self, query: &LogQuery) -> StorageAccessRefund {
        let key = storage_key_of_log(query);
        if query.rw_flag {
            // It is a write

            if self.written_storage_keys.inner().contains_key(&key)
                || self.is_storage_key_free(&key)
            {
                // It is a warm write
                StorageAccessRefund::Warm {
                    ergs: WARM_WRITE_REFUND,
                }
            } else if self.read_storage_keys.inner().contains_key(&key) {
                // If the user has read the value, but never written to it, the user will have to compensate for the write, but not for the read.
                // So the user will pay `STORAGE_ACCESS_COLD_WRITE_COST - STORAGE_ACCESS_COLD_READ_COST` for such write.
                StorageAccessRefund::Warm {
                    ergs: COLD_WRITE_AFTER_WARM_READ_REFUND,
                }
            } else {
                // It is a cold write
                StorageAccessRefund::Cold
            }
        } else if self.read_storage_keys.inner().contains_key(&key)
            || self.is_storage_key_free(&key)
        {
            // It is a warm read
            StorageAccessRefund::Warm {
                ergs: WARM_READ_REFUND,
            }
        } else {
            // It is a cold read
            StorageAccessRefund::Cold
        }
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

    /// Returns storage log queries from current frame where `log.log_query.timestamp >= from_timestamp`.
    pub(crate) fn storage_log_queries_after_timestamp(
        &self,
        from_timestamp: Timestamp,
    ) -> &[Box<StorageLogQuery>] {
        let logs = self.storage_frames_stack.forward().current_frame();

        // Select all of the last elements where `l.log_query.timestamp >= from_timestamp`.
        // Note, that using binary search here is dangerous, because the logs are not sorted by timestamp.
        logs.rsplit(|l| l.log_query.timestamp < from_timestamp.glue_into())
            .next()
            .unwrap_or(&[])
    }

    pub(crate) fn get_final_log_queries(&self) -> Vec<StorageLogQuery> {
        assert_eq!(
            self.storage_frames_stack.len(),
            1,
            "VM finished execution in unexpected state"
        );

        self.storage_frames_stack
            .forward()
            .current_frame()
            .iter()
            .map(|x| **x)
            .collect()
    }

    pub(crate) fn get_size(&self) -> usize {
        let frames_stack_size = self.storage_frames_stack.get_size();
        let paid_changes_size =
            self.paid_changes.inner().len() * std::mem::size_of::<(StorageKey, u32)>();

        frames_stack_size + paid_changes_size
    }

    pub(crate) fn get_history_size(&self) -> usize {
        let storage_size = self.storage.borrow_history(|h| h.len(), 0)
            * std::mem::size_of::<<StorageWrapper<S> as WithHistory>::HistoryRecord>();
        let frames_stack_size = self.storage_frames_stack.get_history_size();
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
    ) -> (LogQuery, PubdataCost) {
        // ```
        // tracing::trace!(
        //     "execute partial query cyc {:?} addr {:?} key {:?}, rw {:?}, wr {:?}, tx {:?}",
        //     _monotonic_cycle_counter,
        //     query.address,
        //     query.key,
        //     query.rw_flag,
        //     query.written_value,
        //     query.tx_number_in_block
        // );
        // ```
        assert!(!query.rollback);

        let storage_key = storage_key_of_log(&query);

        let read_value = if query.aux_byte == TRANSIENT_STORAGE_AUX_BYTE {
            self.transient_storage
                .read_from_transient_storage(&storage_key)
        } else if query.aux_byte == STORAGE_AUX_BYTE {
            if !self.read_storage_keys.inner().contains_key(&storage_key) {
                self.read_storage_keys
                    .insert(storage_key, (), query.timestamp);
            }

            let read_value = self.storage.read_from_storage(&storage_key);
            self.set_initial_value(&storage_key, read_value, query.timestamp);
            read_value
        } else {
            // Just in case
            unreachable!();
        };

        query.read_value = read_value;

        let pubdata_cost = if query.aux_byte == STORAGE_AUX_BYTE && query.rw_flag {
            let current_price = self.base_price_for_write_query(&query);
            let previous_price = self.prepaid_for_write(&storage_key);

            // Note, that the diff may be negative, e.g. in case the new write returns to the original value.
            // The end result is that users pay as much pubdata in total as would have been required to set
            // the slots to their final values.
            // The only case where users may overpay is when a previous transaction ends up with a negative pubdata total.
            let diff = (current_price as i32) - (previous_price as i32);

            self.paid_changes.apply_historic_record(
                HashMapHistoryEvent {
                    key: storage_key,
                    value: Some(current_price),
                },
                query.timestamp,
            );

            PubdataCost(diff)
        } else {
            PubdataCost(0)
        };

        if query.aux_byte == TRANSIENT_STORAGE_AUX_BYTE {
            if query.rw_flag {
                self.write_transient_storage_value(query.into());
            } else {
                self.record_transient_storage_read(query.into());
            }
        } else if query.aux_byte == STORAGE_AUX_BYTE {
            if query.rw_flag {
                self.write_storage_value(query);
            } else {
                self.record_storage_read(query);
            }
        } else {
            // Just in case
            unreachable!();
        }

        self.returned_pubdata_costs
            .apply_historic_record(VectorHistoryEvent::Push(pubdata_cost.0), query.timestamp);

        (query, pubdata_cost)
    }

    // We can evaluate a query cost (or more precisely - get expected refunds)
    // before actually executing query
    fn get_access_refund(
        &mut self, // to avoid any hacks inside, like prefetch
        _monotonic_cycle_counter: u32,
        partial_query: &LogQuery,
    ) -> StorageAccessRefund {
        let refund = if partial_query.aux_byte == TRANSIENT_STORAGE_AUX_BYTE {
            // Any transient access is warm. Also, no refund needs to be provided as it is already cheap
            StorageAccessRefund::Warm { ergs: 0 }
        } else if partial_query.aux_byte == STORAGE_AUX_BYTE {
            self.get_storage_access_refund(partial_query)
        } else {
            unreachable!()
        };

        self.returned_io_refunds.apply_historic_record(
            VectorHistoryEvent::Push(refund.refund()),
            partial_query.timestamp,
        );

        refund
    }

    // Indicate a start of execution frame for rollback purposes
    fn start_frame(&mut self, timestamp: Timestamp) {
        self.storage_frames_stack.push_frame(timestamp);
        self.transient_storage_frames_stack.push_frame(timestamp);
    }

    // Indicate that execution frame went out from the scope, so we can
    // log the history and either rollback immediately or keep records to rollback later
    fn finish_frame(&mut self, timestamp: Timestamp, panicked: bool) {
        // If we panic then we append forward and rollbacks to the forward of parent,
        // otherwise we place rollbacks of child before rollbacks of the parent
        if panicked {
            // perform actual rollback
            for query in self
                .storage_frames_stack
                .rollback()
                .current_frame()
                .iter()
                .rev()
            {
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

                // Now, we also need to roll back the changes to the paid changes.
                let price_for_writing_new_value = self.base_price_for_key_write(&key, read_value);
                self.paid_changes.apply_historic_record(
                    HashMapHistoryEvent {
                        key,
                        value: Some(price_for_writing_new_value),
                    },
                    timestamp,
                );

                // Additional validation that the current value was correct
                // Unwrap is safe because the return value from `write_inner` is the previous value in this leaf.
                // It is impossible to set leaf value to `None`
                assert_eq!(current_value, query.log_query.written_value);
            }
            self.storage_frames_stack
                .move_rollback_to_forward(|_| true, timestamp);

            for query in self
                .transient_storage_frames_stack
                .rollback()
                .current_frame()
                .iter()
                .rev()
            {
                let read_value = if query.rw_flag {
                    query.read_value
                } else {
                    // Having Read logs in rollback is not possible
                    tracing::warn!("Read log in rollback queue {:?}", query);
                    continue;
                };

                let current_value = self.transient_storage.write_to_storage(
                    triplet_to_storage_key(query.shard_id, query.address, query.key),
                    read_value,
                    timestamp,
                );

                assert_eq!(current_value, query.written_value);
            }

            self.transient_storage_frames_stack
                .move_rollback_to_forward(|_| true, timestamp);
        }
        self.storage_frames_stack.merge_frame(timestamp);
        self.transient_storage_frames_stack.merge_frame(timestamp);
    }

    fn start_new_tx(&mut self, timestamp: Timestamp) {
        // Here we perform zeroing out the storage, while maintaining history about it,
        // making it reversible.
        //
        // Note that while the history is preserved, the inner parts are fully cleared out.
        // TODO(X): potentially optimize this function by allowing rollbacks only at the bounds of transactions.

        let current_active_keys = self.transient_storage.clone_vec();
        for (key, current_value) in current_active_keys {
            self.write_transient_storage_value(ReducedTstoreLogQuery {
                // We currently only support rollup shard id
                shard_id: 0,
                address: *key.address(),
                key: h256_to_u256(*key.key()),
                written_value: U256::zero(),
                read_value: current_value,
                rw_flag: true,
                timestamp,
                rollback: false,
            })
        }
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
    use zksync_types::{h256_to_u256, H256};

    use super::*;
    use crate::interface::storage::{InMemoryStorage, StorageView};

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

    enum TestQueryType {
        StorageRead,
        StorageWrite,
    }

    fn make_storage_query(
        key: StorageKey,
        value: U256,
        timestamp: Timestamp,
        query_type: TestQueryType,
    ) -> LogQuery {
        let (rw_flag, aux_byte) = match query_type {
            TestQueryType::StorageRead => (false, STORAGE_AUX_BYTE),
            TestQueryType::StorageWrite => (true, STORAGE_AUX_BYTE),
        };

        LogQuery {
            shard_id: 0,
            address: *key.address(),
            key: h256_to_u256(*key.key()),
            read_value: U256::zero(),
            written_value: value,
            rw_flag,
            aux_byte,
            tx_number_in_block: 0,
            timestamp,
            is_service: false,
            rollback: false,
        }
    }

    #[test]
    fn test_hot_cold_storage() {
        let storage = StorageView::new(InMemoryStorage::default()).to_rc_ptr();

        let mut storage_oracle = StorageOracle::<_, HistoryEnabled>::new(storage.clone());

        storage_oracle.start_frame(Timestamp(0));

        let key1 = StorageKey::new(AccountTreeId::new(Address::default()), H256::default());
        let key2 = StorageKey::new(
            AccountTreeId::new(Address::default()),
            H256::from_low_u64_be(1),
        );

        assert!(
            storage_oracle.get_access_refund(
                0,
                &make_storage_query(
                    key1,
                    U256::default(),
                    Timestamp(0),
                    TestQueryType::StorageRead
                )
            ) == StorageAccessRefund::Cold
        );
        assert!(
            storage_oracle.get_access_refund(
                0,
                &make_storage_query(
                    key1,
                    U256::default(),
                    Timestamp(0),
                    TestQueryType::StorageWrite
                )
            ) == StorageAccessRefund::Cold
        );

        storage_oracle.execute_partial_query(
            0,
            make_storage_query(
                key1,
                U256::default(),
                Timestamp(0),
                TestQueryType::StorageRead,
            ),
        );

        assert!(
            storage_oracle.get_access_refund(
                0,
                &make_storage_query(
                    key1,
                    U256::default(),
                    Timestamp(0),
                    TestQueryType::StorageRead
                )
            ) == StorageAccessRefund::Warm {
                ergs: WARM_READ_REFUND
            }
        );
        assert!(
            storage_oracle.get_access_refund(
                0,
                &make_storage_query(
                    key1,
                    U256::default(),
                    Timestamp(0),
                    TestQueryType::StorageWrite
                )
            ) == StorageAccessRefund::Warm {
                ergs: COLD_WRITE_AFTER_WARM_READ_REFUND
            }
        );

        storage_oracle.execute_partial_query(
            0,
            make_storage_query(
                key1,
                U256::default(),
                Timestamp(0),
                TestQueryType::StorageWrite,
            ),
        );

        assert!(
            storage_oracle.get_access_refund(
                0,
                &make_storage_query(
                    key1,
                    U256::default(),
                    Timestamp(0),
                    TestQueryType::StorageRead
                )
            ) == StorageAccessRefund::Warm {
                ergs: WARM_READ_REFUND
            }
        );
        assert!(
            storage_oracle.get_access_refund(
                0,
                &make_storage_query(
                    key1,
                    U256::default(),
                    Timestamp(0),
                    TestQueryType::StorageWrite
                )
            ) == StorageAccessRefund::Warm {
                ergs: WARM_WRITE_REFUND
            }
        );

        // Now we also test that a write can make both writes and reads warm:
        storage_oracle.execute_partial_query(
            0,
            make_storage_query(
                key2,
                U256::default(),
                Timestamp(0),
                TestQueryType::StorageWrite,
            ),
        );

        assert!(
            storage_oracle.get_access_refund(
                0,
                &make_storage_query(
                    key2,
                    U256::default(),
                    Timestamp(0),
                    TestQueryType::StorageRead
                )
            ) == StorageAccessRefund::Warm {
                ergs: WARM_READ_REFUND
            }
        );
        assert!(
            storage_oracle.get_access_refund(
                0,
                &make_storage_query(
                    key2,
                    U256::default(),
                    Timestamp(0),
                    TestQueryType::StorageWrite
                )
            ) == StorageAccessRefund::Warm {
                ergs: WARM_WRITE_REFUND
            }
        );
    }
}
