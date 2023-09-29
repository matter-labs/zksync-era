use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<vm_m5::errors::VmRevertReasonParsingResult>
    for vm_vm1_3_2::errors::VmRevertReasonParsingResult
{
    fn glue_from(value: vm_m5::errors::VmRevertReasonParsingResult) -> Self {
        Self {
            revert_reason: value.revert_reason.glue_into(),
            original_data: value.original_data,
        }
    }
}

impl GlueFrom<vm_m6::errors::VmRevertReasonParsingResult>
    for vm_vm1_3_2::errors::VmRevertReasonParsingResult
{
    fn glue_from(value: vm_m6::errors::VmRevertReasonParsingResult) -> Self {
        Self {
            revert_reason: value.revert_reason.glue_into(),
            original_data: value.original_data,
        }
    }
}
