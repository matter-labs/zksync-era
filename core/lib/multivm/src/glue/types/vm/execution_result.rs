use crate::glue::{GlueFrom, GlueInto};
use crate::vm_latest::ExecutionResult;

impl GlueFrom<crate::vm_virtual_blocks::ExecutionResult> for ExecutionResult {
    fn glue_from(value: crate::vm_virtual_blocks::ExecutionResult) -> Self {
        match value {
            crate::vm_virtual_blocks::ExecutionResult::Success { output } => {
                ExecutionResult::Success { output }
            }
            crate::vm_virtual_blocks::ExecutionResult::Revert { output } => {
                ExecutionResult::Revert {
                    output: output.glue_into(),
                }
            }
            crate::vm_virtual_blocks::ExecutionResult::Halt { reason } => ExecutionResult::Halt {
                reason: reason.glue_into(),
            },
        }
    }
}
