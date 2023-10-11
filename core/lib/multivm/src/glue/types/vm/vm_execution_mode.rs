use crate::glue::GlueFrom;
use vm_latest::VmExecutionMode;

impl GlueFrom<VmExecutionMode> for vm_virtual_blocks::VmExecutionMode {
    fn glue_from(value: VmExecutionMode) -> Self {
        match value {
            VmExecutionMode::OneTx => vm_virtual_blocks::VmExecutionMode::OneTx,
            VmExecutionMode::Batch => vm_virtual_blocks::VmExecutionMode::Batch,
            VmExecutionMode::Bootloader => vm_virtual_blocks::VmExecutionMode::Bootloader,
        }
    }
}
