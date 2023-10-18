use crate::glue::{GlueFrom, GlueInto};
use crate::vm_latest::{BytecodeCompressionError, VmExecutionResultAndLogs};

impl GlueFrom<crate::vm_virtual_blocks::BytecodeCompressionError> for BytecodeCompressionError {
    fn glue_from(value: crate::vm_virtual_blocks::BytecodeCompressionError) -> Self {
        match value {
            crate::vm_virtual_blocks::BytecodeCompressionError::BytecodeCompressionFailed => {
                Self::BytecodeCompressionFailed
            }
        }
    }
}

impl
    GlueFrom<
        Result<
            crate::vm_virtual_blocks::VmExecutionResultAndLogs,
            crate::vm_virtual_blocks::BytecodeCompressionError,
        >,
    > for Result<VmExecutionResultAndLogs, BytecodeCompressionError>
{
    fn glue_from(
        value: Result<
            crate::vm_virtual_blocks::VmExecutionResultAndLogs,
            crate::vm_virtual_blocks::BytecodeCompressionError,
        >,
    ) -> Self {
        match value {
            Ok(result) => Ok(result.glue_into()),
            Err(err) => Err(err.glue_into()),
        }
    }
}
