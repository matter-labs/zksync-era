use std::collections::HashMap;

use circuit_sequencer_api::sort_storage_access::sort_storage_access_queries;
use zk_evm_1_3_1::aux_structures::{LogQuery, Timestamp};
use zksync_types::{StorageKey, PUBLISH_BYTECODE_OVERHEAD, SYSTEM_CONTEXT_ADDRESS};

use crate::{
    glue::GlueInto,
    interface::VmEvent,
    utils::{bytecode::bytecode_len_in_bytes, glue_log_query},
    vm_m5::{
        oracles::storage::storage_key_of_log, storage::Storage,
        utils::collect_storage_log_queries_after_timestamp, vm_instance::VmInstance,
    },
};

impl<S: Storage> VmInstance<S> {
    pub fn pubdata_published(&self, from_timestamp: Timestamp) -> u32 {
        let storage_writes_pubdata_published = self.pubdata_published_for_writes(from_timestamp);

        let (events, l2_to_l1_logs) =
            self.collect_events_and_l1_logs_after_timestamp(from_timestamp);
        // For the first transaction in L1 batch there may be (it depends on the execution mode) an L2->L1 log
        // that is sent by `SystemContext` in `setNewBlock`. It's a part of the L1 batch pubdata overhead and not the transaction itself.
        let l2_l1_logs_bytes = (l2_to_l1_logs
            .iter()
            .filter(|log| log.sender != SYSTEM_CONTEXT_ADDRESS)
            .count() as u32)
            * zk_evm_1_3_1::zkevm_opcode_defs::system_params::L1_MESSAGE_PUBDATA_BYTES;
        let l2_l1_long_messages_bytes: u32 = VmEvent::extract_long_l2_to_l1_messages(&events)
            .iter()
            .map(|event| event.len() as u32)
            .sum();

        let published_bytecode_bytes: u32 = VmEvent::extract_published_bytecodes(&events)
            .iter()
            .map(|bytecode_hash| bytecode_len_in_bytes(bytecode_hash) + PUBLISH_BYTECODE_OVERHEAD)
            .sum();

        storage_writes_pubdata_published
            + l2_l1_logs_bytes
            + l2_l1_long_messages_bytes
            + published_bytecode_bytes
    }

    fn pubdata_published_for_writes(&self, from_timestamp: Timestamp) -> u32 {
        // This `HashMap` contains how much was already paid for every slot that was paid during the last tx execution.
        // For the slots that weren't paid during the last tx execution we can just use
        // `self.state.storage.paid_changes.inner().get(&key)` to get how much it was paid before.
        let pre_paid_before_tx_map: HashMap<StorageKey, u32> = self
            .state
            .storage
            .paid_changes
            .history()
            .iter()
            .rev()
            .take_while(|history_elem| history_elem.0 >= from_timestamp)
            .map(|history_elem| (history_elem.1.key, history_elem.1.value.unwrap_or(0)))
            .collect();
        let pre_paid_before_tx = |key: &StorageKey| -> u32 {
            if let Some(pre_paid) = pre_paid_before_tx_map.get(key) {
                *pre_paid
            } else {
                self.state
                    .storage
                    .paid_changes
                    .inner()
                    .get(key)
                    .copied()
                    .unwrap_or(0)
            }
        };

        let storage_logs = collect_storage_log_queries_after_timestamp(
            &self
                .state
                .storage
                .frames_stack
                .inner()
                .current_frame()
                .forward,
            from_timestamp,
        );

        // To allow calling the `vm-1.3.3`s. method, the `v1.3.1`'s `LogQuery` has to be converted
        // to the `vm-1.3.3`'s `LogQuery`. Then, we need to convert it back.
        let deduplicated_logs: Vec<LogQuery> = sort_storage_access_queries(
            storage_logs.iter().map(|log| glue_log_query(log.log_query)),
        )
        .1
        .into_iter()
        .map(glue_log_query)
        .collect();

        deduplicated_logs
            .into_iter()
            .filter_map(|log| {
                if log.rw_flag {
                    let key = storage_key_of_log(&log);
                    let pre_paid = pre_paid_before_tx(&key);
                    let to_pay_by_user = self.state.storage.base_price_for_write(&log.glue_into());

                    if to_pay_by_user > pre_paid {
                        Some(to_pay_by_user - pre_paid)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .sum()
    }
}
