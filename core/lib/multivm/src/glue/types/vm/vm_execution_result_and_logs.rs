use crate::glue::{GlueFrom, GlueInto};
use vm_latest::VmExecutionResultAndLogs;

impl GlueFrom<vm_virtual_blocks::VmExecutionResultAndLogs> for VmExecutionResultAndLogs {
    fn glue_from(value: vm_virtual_blocks::VmExecutionResultAndLogs) -> Self {
        Self {
            result: value.result.glue_into(),
            logs: value.logs,
            statistics: value.statistics.glue_into(),
            refunds: value.refunds.glue_into(),
        }
    }
}
