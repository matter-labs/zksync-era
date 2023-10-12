use crate::glue::{GlueFrom, GlueInto};
use vm_latest::ExecutionResult;

impl GlueFrom<vm_virtual_blocks::ExecutionResult> for ExecutionResult {
    fn glue_from(value: vm_virtual_blocks::ExecutionResult) -> Self {
        match value {
            vm_virtual_blocks::ExecutionResult::Success { output } => {
                ExecutionResult::Success { output }
            }
            vm_virtual_blocks::ExecutionResult::Revert { output } => ExecutionResult::Revert {
                output: output.glue_into(),
            },
            vm_virtual_blocks::ExecutionResult::Halt { reason } => ExecutionResult::Halt {
                reason: reason.glue_into(),
            },
        }
    }
}
