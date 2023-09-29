use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<vm_m5::VmBlockResult> for vm_vm1_3_2::VmBlockResult {
    fn glue_from(value: vm_m5::VmBlockResult) -> Self {
        Self {
            full_result: value.full_result.glue_into(),
            block_tip_result: value.block_tip_result.glue_into(),
        }
    }
}

impl GlueFrom<vm_m6::VmBlockResult> for vm_vm1_3_2::VmBlockResult {
    fn glue_from(value: vm_m6::VmBlockResult) -> Self {
        Self {
            full_result: value.full_result.glue_into(),
            block_tip_result: value.block_tip_result.glue_into(),
        }
    }
}
