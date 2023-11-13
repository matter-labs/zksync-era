use std::collections::HashMap;

use crate::vm_m6::storage::{Storage, StoragePtr};

use crate::glue::GlueInto;
use crate::vm_m6::history_recorder::{
    AppDataFrameManagerWithHistory, HashMapHistoryEvent, HistoryEnabled, HistoryMode,
    HistoryRecorder, StorageWrapper, WithHistory,
};

use zk_evm_1_3_1::abstractions::RefundedAmounts;
use zk_evm_1_3_1::zkevm_opcode_defs::system_params::INITIAL_STORAGE_WRITE_PUBDATA_BYTES;
use zk_evm_1_3_1::{
    abstractions::{RefundType, Storage as VmStorageOracle},
    aux_structures::{LogQuery, Timestamp},
};
use zksync_types::utils::storage_key_for_eth_balance;
use zksync_types::{
    AccountTreeId, Address, StorageKey, StorageLogQuery, StorageLogQueryType, BOOTLOADER_ADDRESS,
    U256,
};
use zksync_utils::u256_to_h256;

use super::OracleWithHistory;

// While the storage does not support different shards, it was decided to write the
// code of the StorageOracle with the shard parameters in mind.
pub fn triplet_to_storage_key(_shard_id: u8, address: Address, key: U256) -> StorageKey {
    StorageKey::new(AccountTreeId::new(address), u256_to_h256(key))
}

pub fn storage_key_of_log(query: &LogQuery) -> StorageKey {
    triplet_to_storage_key(query.shard_id, query.address, query.key)
}

#[derive(Debug)]
pub struct StorageOracle<S: Storage, H: HistoryMode> {
    // Access to the persistent storage. Please note that it
    // is used only for read access. All the actual writes happen
    // after the execution ended.
    pub storage: HistoryRecorder<StorageWrapper<S>, H>,

    pub frames_stack: AppDataFrameManagerWithHistory<StorageLogQuery, H>,

    // The changes that have been paid for in previous transactions.
    // It is a mapping from storage key to the number of *bytes* that was paid by the user
    // to cover this slot.
    // `paid_changes` history is necessary
    pub paid_changes: HistoryRecorder<HashMap<StorageKey, u32>, HistoryEnabled>,
}

impl<S: Storage> OracleWithHistory for StorageOracle<S, HistoryEnabled> {
    fn rollback_to_timestamp(&mut self, timestamp: Timestamp) {
        self.frames_stack.rollback_to_timestamp(timestamp);
        self.storage.rollback_to_timestamp(timestamp);
        self.paid_changes.rollback_to_timestamp(timestamp);
    }
}

impl<S: Storage, H: HistoryMode> StorageOracle<S, H> {
    pub fn new(storage: StoragePtr<S>) -> Self {
        Self {
            storage: HistoryRecorder::from_inner(StorageWrapper::new(storage)),
            frames_stack: Default::default(),
            paid_changes: Default::default(),
        }
    }

    pub fn delete_history(&mut self) {
        self.frames_stack.delete_history();
        self.storage.delete_history();
        self.paid_changes.delete_history();
    }

    fn is_storage_key_free(&self, key: &StorageKey) -> bool {
        key.address() == &zksync_system_constants::SYSTEM_CONTEXT_ADDRESS
            || *key == storage_key_for_eth_balance(&BOOTLOADER_ADDRESS)
    }

    pub fn read_value(&mut self, mut query: LogQuery) -> LogQuery {
        let key = triplet_to_storage_key(query.shard_id, query.address, query.key);
        let current_value = self.storage.read_from_storage(&key);

        query.read_value = current_value;

        self.frames_stack.push_forward(
            StorageLogQuery {
                log_query: query.glue_into(),
                log_type: StorageLogQueryType::Read,
            },
            query.timestamp,
        );

        query
    }

    pub fn write_value(&mut self, mut query: LogQuery) -> LogQuery {
        let key = triplet_to_storage_key(query.shard_id, query.address, query.key);
        let current_value =
            self.storage
                .write_to_storage(key, query.written_value, query.timestamp);

        let log_query_type = if self.storage.get_ptr().borrow_mut().is_write_initial(&key) {
            StorageLogQueryType::InitialWrite
        } else {
            StorageLogQueryType::RepeatedWrite
        };

        query.read_value = current_value;

        let mut storage_log_query = StorageLogQuery {
            log_query: query.glue_into(),
            log_type: log_query_type,
        };
        self.frames_stack
            .push_forward(storage_log_query, query.timestamp);
        storage_log_query.log_query.rollback = true;
        self.frames_stack
            .push_rollback(storage_log_query, query.timestamp);
        storage_log_query.log_query.rollback = false;

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

    pub(crate) fn base_price_for_write(&self, query: &LogQuery) -> u32 {
        let storage_key = storage_key_of_log(query);

        if self.is_storage_key_free(&storage_key) {
            return 0;
        }

        let is_initial = self
            .storage
            .get_ptr()
            .borrow_mut()
            .is_write_initial(&storage_key);

        get_pubdata_price_bytes(query, is_initial)
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

    pub fn get_size(&self) -> usize {
        let frames_stack_size = self.frames_stack.get_size();
        let paid_changes_size =
            self.paid_changes.inner().len() * std::mem::size_of::<(StorageKey, u32)>();

        frames_stack_size + paid_changes_size
    }

    pub fn get_history_size(&self) -> usize {
        let storage_size = self.storage.borrow_history(|h| h.len(), 0)
            * std::mem::size_of::<<StorageWrapper<S> as WithHistory>::HistoryRecord>();
        let frames_stack_size = self.frames_stack.get_history_size();
        let paid_changes_size = self.paid_changes.borrow_history(|h| h.len(), 0)
            * std::mem::size_of::<<HashMap<StorageKey, u32> as WithHistory>::HistoryRecord>();
        storage_size + frames_stack_size + paid_changes_size
    }
}

impl<S: Storage, H: HistoryMode> VmStorageOracle for StorageOracle<S, H> {
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
            let to_pay_by_user = self.base_price_for_write(&query);
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
        let price_to_pay = self.value_update_price(partial_query);

        RefundType::RepeatedWrite(RefundedAmounts {
            ergs: 0,
            // `INITIAL_STORAGE_WRITE_PUBDATA_BYTES` is the default amount of pubdata bytes the user pays for.
            pubdata_bytes: (INITIAL_STORAGE_WRITE_PUBDATA_BYTES as u32) - price_to_pay,
        })
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
                    StorageLogQueryType::Read => {
                        // Having Read logs in rollback is not possible
                        tracing::warn!("Read log in rollback queue {:?}", query);
                        continue;
                    }
                    StorageLogQueryType::InitialWrite | StorageLogQueryType::RepeatedWrite => {
                        query.log_query.read_value
                    }
                };

                let LogQuery { written_value, .. } = query.log_query.glue_into();
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
                // Unwrap is safe because the return value from write_inner is the previous value in this leaf.
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
        zk_evm_1_3_1::zkevm_opcode_defs::system_params::INITIAL_STORAGE_WRITE_PUBDATA_BYTES as u32
    } else {
        zk_evm_1_3_1::zkevm_opcode_defs::system_params::REPEATED_STORAGE_WRITE_PUBDATA_BYTES as u32
    }
}
