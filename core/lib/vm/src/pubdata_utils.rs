use crate::oracles::storage::storage_key_of_log;
use crate::utils::collect_storage_log_queries_after_timestamp;
use crate::VmInstance;
use std::collections::HashMap;
use zk_evm::aux_structures::Timestamp;
use zksync_types::event::{extract_long_l2_to_l1_messages, extract_published_bytecodes};
use zksync_types::StorageKey;
use zksync_utils::bytecode::bytecode_len_in_bytes;

impl<'a> VmInstance<'a> {
    pub fn pubdata_used(&self, from_timestamp: Timestamp) -> u32 {
        let storage_writes_pubdata_used = self.pubdata_used_for_writes(from_timestamp);

        let (events, l2_to_l1_logs) =
            self.collect_events_and_l1_logs_after_timestamp(from_timestamp);
        let l2_l1_logs_bytes = (l2_to_l1_logs.len() as u32)
            * zk_evm::zkevm_opcode_defs::system_params::L1_MESSAGE_PUBDATA_BYTES;
        let l2_l1_long_messages_bytes: u32 = extract_long_l2_to_l1_messages(&events)
            .iter()
            .map(|event| event.len() as u32)
            .sum();

        let published_bytecode_bytes: u32 = extract_published_bytecodes(&events)
            .iter()
            .map(|bytecodehash| bytecode_len_in_bytes(*bytecodehash) as u32)
            .sum();

        storage_writes_pubdata_used
            + l2_l1_logs_bytes
            + l2_l1_long_messages_bytes
            + published_bytecode_bytes
    }

    fn pubdata_used_for_writes(&self, from_timestamp: Timestamp) -> u32 {
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
        let (_, deduplicated_logs) =
            zksync_types::log_query_sorter::sort_storage_access_queries(&storage_logs);

        deduplicated_logs
            .into_iter()
            .filter_map(|log| {
                if log.log_query.rw_flag {
                    let key = storage_key_of_log(&log.log_query);
                    let pre_paid = pre_paid_before_tx(&key);
                    let to_pay_by_user = self.state.storage.base_price_for_write(&log.log_query);

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
