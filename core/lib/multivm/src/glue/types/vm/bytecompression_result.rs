use crate::glue::{GlueFrom, GlueInto};
use vm_latest::{BytecodeCompressionError, VmExecutionResultAndLogs};

impl GlueFrom<vm_virtual_blocks::BytecodeCompressionError> for BytecodeCompressionError {
    fn glue_from(value: vm_virtual_blocks::BytecodeCompressionError) -> Self {
        match value {
            vm_virtual_blocks::BytecodeCompressionError::BytecodeCompressionFailed => {
                Self::BytecodeCompressionFailed
            }
        }
    }
}

impl
    GlueFrom<
        Result<
            vm_virtual_blocks::VmExecutionResultAndLogs,
            vm_virtual_blocks::BytecodeCompressionError,
        >,
    > for Result<VmExecutionResultAndLogs, BytecodeCompressionError>
{
    fn glue_from(
        value: Result<
            vm_virtual_blocks::VmExecutionResultAndLogs,
            vm_virtual_blocks::BytecodeCompressionError,
        >,
    ) -> Self {
        match value {
            Ok(result) => Ok(result.glue_into()),
            Err(err) => Err(err.glue_into()),
        }
    }
}
