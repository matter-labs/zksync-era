use std::collections::HashMap;

use itertools::Itertools;
use zk_evm_1_4_1::{
    abstractions::EventSink,
    aux_structures::{LogQuery, Timestamp},
    reference_impls::event_sink::EventMessage,
    zkevm_opcode_defs::system_params::{
        BOOTLOADER_FORMAL_ADDRESS, EVENT_AUX_BYTE, L1_MESSAGE_AUX_BYTE,
    },
};
use zksync_types::U256;

use crate::vm_1_4_2::old_vm::{
    history_recorder::{AppDataFrameManagerWithHistory, HistoryEnabled, HistoryMode},
    oracles::OracleWithHistory,
};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InMemoryEventSink<H: HistoryMode> {
    frames_stack: AppDataFrameManagerWithHistory<Box<LogQuery>, H>,
}

impl OracleWithHistory for InMemoryEventSink<HistoryEnabled> {
    fn rollback_to_timestamp(&mut self, timestamp: Timestamp) {
        self.frames_stack.rollback_to_timestamp(timestamp);
    }
}

// as usual, if we rollback the current frame then we apply changes to storage immediately,
// otherwise we carry rollbacks to the parent's frames

impl<H: HistoryMode> InMemoryEventSink<H> {
    pub fn flatten(&self) -> (Vec<LogQuery>, Vec<EventMessage>, Vec<EventMessage>) {
        assert_eq!(
            self.frames_stack.len(),
            1,
            "there must exist an initial keeper frame"
        );
        // we forget rollbacks as we have finished the execution and can just apply them
        let history = self.frames_stack.forward().current_frame();

        let (events, l1_messages) = Self::events_and_l1_messages_from_history(history);
        let events_logs = Self::events_logs_from_history(history);

        (events_logs, events, l1_messages)
    }

    pub fn get_log_queries(&self) -> usize {
        self.frames_stack.forward().current_frame().len()
    }

    /// Returns the log queries in the current frame where `log_query.timestamp >= from_timestamp`.
    pub fn log_queries_after_timestamp(&self, from_timestamp: Timestamp) -> &[Box<LogQuery>] {
        let events = self.frames_stack.forward().current_frame();

        // Select all of the last elements where `e.timestamp >= from_timestamp`.
        // Note, that using binary search here is dangerous, because the logs are not sorted by timestamp.
        events
            .rsplit(|e| e.timestamp < from_timestamp)
            .next()
            .unwrap_or(&[])
    }

    pub fn get_events_and_l2_l1_logs_after_timestamp(
        &self,
        from_timestamp: Timestamp,
    ) -> (Vec<EventMessage>, Vec<EventMessage>) {
        Self::events_and_l1_messages_from_history(self.log_queries_after_timestamp(from_timestamp))
    }

    fn events_logs_from_history(history: &[Box<LogQuery>]) -> Vec<LogQuery> {
        // Filter out all the L2->L1 logs and leave only events
        let mut events = history
            .iter()
            .filter_map(|log_query| (log_query.aux_byte == EVENT_AUX_BYTE).then_some(**log_query))
            .collect_vec();

        // Sort the events by timestamp and rollback flag, basically ensuring that
        // if an event has been rolled back, the original event and its rollback will be put together
        events.sort_by_key(|log| (log.timestamp, log.rollback));

        let mut stack = Vec::<LogQuery>::new();
        let mut net_history = vec![];
        for el in events.iter() {
            assert_eq!(el.shard_id, 0, "only rollup shard is supported");
            if stack.is_empty() {
                assert!(!el.rollback);
                stack.push(*el);
            } else {
                // we can always pop as it's either one to add to queue, or discard
                let previous = stack.pop().unwrap();
                if previous.timestamp == el.timestamp {
                    // Only rollback can have the same timestamp, so here we do nothing and simply
                    // double check the invariants
                    assert!(!previous.rollback);
                    assert!(el.rollback);
                    assert!(previous.rw_flag);
                    assert!(el.rw_flag);
                    assert_eq!(previous.tx_number_in_block, el.tx_number_in_block);
                    assert_eq!(previous.shard_id, el.shard_id);
                    assert_eq!(previous.address, el.address);
                    assert_eq!(previous.key, el.key);
                    assert_eq!(previous.written_value, el.written_value);
                    assert_eq!(previous.is_service, el.is_service);
                    continue;
                } else {
                    // The event on the stack has not been rolled back. It must be a different event,
                    // with a different timestamp.
                    assert!(!el.rollback);
                    stack.push(*el);

                    // cleanup some fields
                    // flags are conventions
                    let sorted_log_query = LogQuery {
                        timestamp: Timestamp(0),
                        tx_number_in_block: previous.tx_number_in_block,
                        aux_byte: 0,
                        shard_id: previous.shard_id,
                        address: previous.address,
                        key: previous.key,
                        read_value: U256::zero(),
                        written_value: previous.written_value,
                        rw_flag: false,
                        rollback: false,
                        is_service: previous.is_service,
                    };

                    net_history.push(sorted_log_query);
                }
            }
        }

        // In case the stack is non-empty, then the last element of it has not been rolled back.
        if let Some(previous) = stack.pop() {
            // cleanup some fields
            // flags are conventions
            let sorted_log_query = LogQuery {
                timestamp: Timestamp(0),
                tx_number_in_block: previous.tx_number_in_block,
                aux_byte: 0,
                shard_id: previous.shard_id,
                address: previous.address,
                key: previous.key,
                read_value: U256::zero(),
                written_value: previous.written_value,
                rw_flag: false,
                rollback: false,
                is_service: previous.is_service,
            };

            net_history.push(sorted_log_query);
        }

        net_history
    }

    fn events_and_l1_messages_from_history(
        history: &[Box<LogQuery>],
    ) -> (Vec<EventMessage>, Vec<EventMessage>) {
        let mut tmp = HashMap::<u32, LogQuery>::with_capacity(history.len());

        // note that we only use "forward" part and discard the rollbacks at the end,
        // since if rollbacks of parents were not appended anywhere we just still keep them
        for el in history {
            // we are time ordered here in terms of rollbacks
            #[allow(clippy::map_entry)]
            if tmp.contains_key(&el.timestamp.0) {
                assert!(el.rollback);
                tmp.remove(&el.timestamp.0);
            } else {
                assert!(!el.rollback);
                tmp.insert(el.timestamp.0, **el);
            }
        }

        // naturally sorted by timestamp
        let mut keys: Vec<_> = tmp.keys().cloned().collect();
        keys.sort_unstable();

        let mut events = vec![];
        let mut l1_messages = vec![];

        for k in keys.into_iter() {
            let el = tmp.remove(&k).unwrap();
            let LogQuery {
                shard_id,
                is_service,
                tx_number_in_block,
                address,
                key,
                written_value,
                aux_byte,
                ..
            } = el;

            let event = EventMessage {
                shard_id,
                is_first: is_service,
                tx_number_in_block,
                address,
                key,
                value: written_value,
            };

            if aux_byte == EVENT_AUX_BYTE {
                events.push(event);
            } else {
                l1_messages.push(event);
            }
        }

        (events, l1_messages)
    }

    pub(crate) fn get_size(&self) -> usize {
        self.frames_stack.get_size()
    }

    pub fn get_history_size(&self) -> usize {
        self.frames_stack.get_history_size()
    }

    pub fn delete_history(&mut self) {
        self.frames_stack.delete_history();
    }
}

impl<H: HistoryMode> EventSink for InMemoryEventSink<H> {
    // when we enter a new frame we should remember all our current applications and rollbacks
    // when we exit the current frame then if we did panic we should concatenate all current
    // forward and rollback cases

    fn add_partial_query(&mut self, _monotonic_cycle_counter: u32, mut query: LogQuery) {
        assert!(query.rw_flag);
        assert!(query.aux_byte == EVENT_AUX_BYTE || query.aux_byte == L1_MESSAGE_AUX_BYTE);
        assert!(!query.rollback);

        // just append to rollbacks and a full history

        self.frames_stack
            .push_forward(Box::new(query), query.timestamp);
        // we do not need it explicitly here, but let's be consistent with circuit counterpart
        query.rollback = true;
        self.frames_stack
            .push_rollback(Box::new(query), query.timestamp);
    }

    fn start_frame(&mut self, timestamp: Timestamp) {
        self.frames_stack.push_frame(timestamp)
    }

    fn finish_frame(&mut self, panicked: bool, timestamp: Timestamp) {
        // if we panic then we append forward and rollbacks to the forward of parent,
        // otherwise we place rollbacks of child before rollbacks of the parent
        if panicked {
            self.frames_stack.move_rollback_to_forward(
                |q| q.address != *BOOTLOADER_FORMAL_ADDRESS || q.aux_byte != EVENT_AUX_BYTE,
                timestamp,
            );
        }
        self.frames_stack.merge_frame(timestamp);
    }
}
