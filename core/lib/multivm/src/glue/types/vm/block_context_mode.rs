use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<vm_vm1_3_2::vm_with_bootloader::BlockContextMode>
    for vm_m5::vm_with_bootloader::BlockContextMode
{
    fn glue_from(value: vm_vm1_3_2::vm_with_bootloader::BlockContextMode) -> Self {
        match value {
            vm_vm1_3_2::vm_with_bootloader::BlockContextMode::NewBlock(
                derived,
                prev_block_hash,
            ) => {
                let derived = derived.glue_into();
                Self::NewBlock(derived, prev_block_hash)
            }
            vm_vm1_3_2::vm_with_bootloader::BlockContextMode::OverrideCurrent(derived) => {
                let derived = derived.glue_into();
                Self::OverrideCurrent(derived)
            }
        }
    }
}

impl GlueFrom<vm_vm1_3_2::vm_with_bootloader::BlockContextMode>
    for vm_m6::vm_with_bootloader::BlockContextMode
{
    fn glue_from(value: vm_vm1_3_2::vm_with_bootloader::BlockContextMode) -> Self {
        match value {
            vm_vm1_3_2::vm_with_bootloader::BlockContextMode::NewBlock(
                derived,
                prev_block_hash,
            ) => {
                let derived = derived.glue_into();
                Self::NewBlock(derived, prev_block_hash)
            }
            vm_vm1_3_2::vm_with_bootloader::BlockContextMode::OverrideCurrent(derived) => {
                let derived = derived.glue_into();
                Self::OverrideCurrent(derived)
            }
        }
    }
}
