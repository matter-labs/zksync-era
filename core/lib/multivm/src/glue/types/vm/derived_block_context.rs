use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<vm_vm1_3_2::vm_with_bootloader::DerivedBlockContext>
    for vm_m5::vm_with_bootloader::DerivedBlockContext
{
    fn glue_from(value: vm_vm1_3_2::vm_with_bootloader::DerivedBlockContext) -> Self {
        Self {
            context: value.context.glue_into(),
            base_fee: value.base_fee,
        }
    }
}

impl GlueFrom<vm_vm1_3_2::vm_with_bootloader::DerivedBlockContext>
    for vm_m6::vm_with_bootloader::DerivedBlockContext
{
    fn glue_from(value: vm_vm1_3_2::vm_with_bootloader::DerivedBlockContext) -> Self {
        Self {
            context: value.context.glue_into(),
            base_fee: value.base_fee,
        }
    }
}
