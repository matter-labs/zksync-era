use zk_evm_1_5_2::aux_structures::Timestamp;
use zksync_types::l2_to_l1_log::{L2ToL1Log, SystemL2ToL1Log, UserL2ToL1Log};

use crate::{
    glue::GlueInto,
    interface::{storage::WriteStorage, VmEvent, VmExecutionLogs},
    utils::events::extract_l2tol1logs_from_l1_messenger,
    vm_latest::{old_vm::utils::precompile_calls_count_after_timestamp, utils::logs, vm::Vm},
    HistoryMode,
};

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

        let (events, system_l2_to_l1_logs) =
            self.collect_events_and_l1_system_logs_after_timestamp(from_timestamp);

        let log_queries = self
            .state
            .event_sink
            .log_queries_after_timestamp(from_timestamp);

        let precompile_calls_count = precompile_calls_count_after_timestamp(
            self.state.precompiles_processor.timestamp_history.inner(),
            from_timestamp,
        );

        let user_logs = extract_l2tol1logs_from_l1_messenger(&events);

        let total_log_queries_count =
            storage_logs_count + log_queries.len() + precompile_calls_count;

        VmExecutionLogs {
            storage_logs: storage_logs.into_iter().map(GlueInto::glue_into).collect(),
            events,
            user_l2_to_l1_logs: user_logs
                .into_iter()
                .map(|log| UserL2ToL1Log(log.into()))
                .collect(),
            system_l2_to_l1_logs: system_l2_to_l1_logs
                .into_iter()
                .map(SystemL2ToL1Log)
                .collect(),
            total_log_queries_count,
        }
    }

    pub(crate) fn collect_events_and_l1_system_logs_after_timestamp(
        &self,
        from_timestamp: Timestamp,
    ) -> (Vec<VmEvent>, Vec<L2ToL1Log>) {
        logs::collect_events_and_l1_system_logs_after_timestamp(
            &self.state,
            &self.batch_env,
            from_timestamp.glue_into(),
        )
    }
}
