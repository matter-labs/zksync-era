use zk_evm_1_3_3::aux_structures::Timestamp;
use zksync_state::WriteStorage;

use crate::interface::types::outputs::VmExecutionLogs;
use crate::HistoryMode;
use zksync_types::l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log};
use zksync_types::VmEvent;

use crate::vm_virtual_blocks::old_vm::events::merge_events;
use crate::vm_virtual_blocks::old_vm::utils::precompile_calls_count_after_timestamp;
use crate::vm_virtual_blocks::vm::Vm;

impl<S: WriteStorage, H: HistoryMode> Vm<S, H> {
    pub(crate) fn collect_execution_logs_after_timestamp(
        &self,
        from_timestamp: Timestamp,
    ) -> VmExecutionLogs {
        let storage_logs: Vec<_> = self
            .state
            .storage
            .storage_log_queries_after_timestamp(from_timestamp)
            .iter()
            .map(|log| **log)
            .collect();
        let storage_logs_count = storage_logs.len();

        let (events, l2_to_l1_logs) =
            self.collect_events_and_l1_logs_after_timestamp(from_timestamp);

        let log_queries = self
            .state
            .event_sink
            .log_queries_after_timestamp(from_timestamp);

        let precompile_calls_count = precompile_calls_count_after_timestamp(
            self.state.precompiles_processor.timestamp_history.inner(),
            from_timestamp,
        );

        let total_log_queries_count =
            storage_logs_count + log_queries.len() + precompile_calls_count;
        VmExecutionLogs {
            storage_logs,
            events,
            user_l2_to_l1_logs: l2_to_l1_logs.into_iter().map(UserL2ToL1Log).collect(),
            system_l2_to_l1_logs: vec![],
            total_log_queries_count,
        }
    }

    pub(crate) fn collect_events_and_l1_logs_after_timestamp(
        &self,
        from_timestamp: Timestamp,
    ) -> (Vec<VmEvent>, Vec<L2ToL1Log>) {
        let (raw_events, l1_messages) = self
            .state
            .event_sink
            .get_events_and_l2_l1_logs_after_timestamp(from_timestamp);
        let events = merge_events(raw_events)
            .into_iter()
            .map(|e| e.into_vm_event(self.batch_env.number))
            .collect();
        (events, l1_messages.into_iter().map(Into::into).collect())
    }
}
