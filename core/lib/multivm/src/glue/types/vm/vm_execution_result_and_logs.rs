use crate::glue::{GlueFrom, GlueInto};
use vm_latest::{VmExecutionLogs, VmExecutionResultAndLogs};

impl GlueFrom<vm_virtual_blocks::VmExecutionResultAndLogs> for VmExecutionResultAndLogs {
    fn glue_from(value: vm_virtual_blocks::VmExecutionResultAndLogs) -> Self {
        Self {
            result: value.result.glue_into(),
            logs: VmExecutionLogs {
                storage_logs: value.logs.storage_logs,
                events: value.logs.events,
                user_l2_to_l1_logs: value.logs.l2_to_l1_logs,
                system_l2_to_l1_logs: vec![],
                total_log_queries_count: value.logs.total_log_queries_count,
            },
            statistics: value.statistics.glue_into(),
            refunds: value.refunds.glue_into(),
        }
    }
}
