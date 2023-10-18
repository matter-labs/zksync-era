use crate::glue::{GlueFrom, GlueInto};
use crate::vm_latest::VmExecutionResultAndLogs;

impl GlueFrom<crate::vm_virtual_blocks::VmExecutionResultAndLogs> for VmExecutionResultAndLogs {
    fn glue_from(value: crate::vm_virtual_blocks::VmExecutionResultAndLogs) -> Self {
        Self {
            result: value.result.glue_into(),
            logs: value.logs,
            statistics: value.statistics.glue_into(),
            refunds: value.refunds.glue_into(),
        }
    }
}
