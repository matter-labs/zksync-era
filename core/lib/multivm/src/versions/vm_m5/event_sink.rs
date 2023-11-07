use crate::vm_m5::{oracles::OracleWithHistory, utils::collect_log_queries_after_timestamp};
use std::collections::HashMap;
use zk_evm_1_3_1::{
    abstractions::EventSink,
    aux_structures::{LogQuery, Timestamp},
    reference_impls::event_sink::{ApplicationData, EventMessage},
    zkevm_opcode_defs::system_params::{
        BOOTLOADER_FORMAL_ADDRESS, EVENT_AUX_BYTE, L1_MESSAGE_AUX_BYTE,
    },
};

use crate::vm_m5::history_recorder::AppDataFrameManagerWithHistory;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct InMemoryEventSink {
    pub frames_stack: AppDataFrameManagerWithHistory<LogQuery>,
}

impl OracleWithHistory for InMemoryEventSink {
    fn rollback_to_timestamp(&mut self, timestamp: Timestamp) {
        self.frames_stack.rollback_to_timestamp(timestamp);
    }

    fn delete_history(&mut self) {
        self.frames_stack.delete_history();
    }
}

// as usual, if we rollback the current frame then we apply changes to storage immediately,
// otherwise we carry rollbacks to the parent's frames

impl InMemoryEventSink {
    pub fn flatten(&self) -> (Vec<LogQuery>, Vec<EventMessage>, Vec<EventMessage>) {
        assert_eq!(
            self.frames_stack.inner().len(),
            1,
            "there must exist an initial keeper frame"
        );
        let full_history = self.frames_stack.inner().current_frame().clone();
        // we forget rollbacks as we have finished the execution and can just apply them
        let ApplicationData {
            forward,
            rollbacks: _,
        } = full_history;
        let history = forward.clone();
        let (events, l1_messages) = Self::events_and_l1_messages_from_history(forward);
        (history, events, l1_messages)
    }

    pub fn get_log_queries(&self) -> usize {
        let history = &self.frames_stack.inner().current_frame().forward;
        history.len()
    }

    pub fn get_events_and_l2_l1_logs_after_timestamp(
        &self,
        from_timestamp: Timestamp,
    ) -> (Vec<EventMessage>, Vec<EventMessage>) {
        let history = collect_log_queries_after_timestamp(
            &self.frames_stack.inner().current_frame().forward,
            from_timestamp,
        );
        Self::events_and_l1_messages_from_history(history)
    }

    fn events_and_l1_messages_from_history(
        history: Vec<LogQuery>,
    ) -> (Vec<EventMessage>, Vec<EventMessage>) {
        let mut tmp = HashMap::<u32, LogQuery>::with_capacity(history.len());

        // note that we only use "forward" part and discard the rollbacks at the end,
        // since if rollbacks of parents were not appended anywhere we just still keep them
        for el in history.into_iter() {
            // we are time ordered here in terms of rollbacks
            if tmp.get(&el.timestamp.0).is_some() {
                assert!(el.rollback);
                tmp.remove(&el.timestamp.0);
            } else {
                assert!(!el.rollback);
                tmp.insert(el.timestamp.0, el);
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
}

impl EventSink for InMemoryEventSink {
    // when we enter a new frame we should remember all our current applications and rollbacks
    // when we exit the current frame then if we did panic we should concatenate all current
    // forward and rollback cases

    fn add_partial_query(&mut self, _monotonic_cycle_counter: u32, mut query: LogQuery) {
        assert!(query.rw_flag);
        assert!(query.aux_byte == EVENT_AUX_BYTE || query.aux_byte == L1_MESSAGE_AUX_BYTE);
        assert!(!query.rollback);
        // just append to rollbacks and a full history

        self.frames_stack.push_forward(query, query.timestamp);
        // we do not need it explicitly here, but let's be consistent with circuit counterpart
        query.rollback = true;
        self.frames_stack.push_rollback(query, query.timestamp);
    }

    fn start_frame(&mut self, timestamp: Timestamp) {
        self.frames_stack.push_frame(timestamp)
    }

    fn finish_frame(&mut self, panicked: bool, timestamp: Timestamp) {
        // if we panic then we append forward and rollbacks to the forward of parent,
        // otherwise we place rollbacks of child before rollbacks of the parent
        let ApplicationData { forward, rollbacks } = self.frames_stack.drain_frame(timestamp);
        if panicked {
            for query in forward {
                self.frames_stack.push_forward(query, timestamp);
            }
            for query in rollbacks.into_iter().rev().filter(|q| {
                // As of now, the bootloader only emits debug logs
                // for events, so we keep them here for now.
                // They will be cleared on the server level.
                q.address != *BOOTLOADER_FORMAL_ADDRESS || q.aux_byte != EVENT_AUX_BYTE
            }) {
                self.frames_stack.push_forward(query, timestamp);
            }
        } else {
            for query in forward {
                self.frames_stack.push_forward(query, timestamp);
            } // we need to prepend rollbacks. No reverse here, as we do not care yet!
            for query in rollbacks {
                self.frames_stack.push_rollback(query, timestamp);
            }
        }
    }
}
