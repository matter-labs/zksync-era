use crate::glue::GlueFrom;
use crate::vm_latest::VmExecutionMode;

impl GlueFrom<VmExecutionMode> for crate::vm_virtual_blocks::VmExecutionMode {
    fn glue_from(value: VmExecutionMode) -> Self {
        match value {
            VmExecutionMode::OneTx => crate::vm_virtual_blocks::VmExecutionMode::OneTx,
            VmExecutionMode::Batch => crate::vm_virtual_blocks::VmExecutionMode::Batch,
            VmExecutionMode::Bootloader => crate::vm_virtual_blocks::VmExecutionMode::Bootloader,
        }
    }
}
