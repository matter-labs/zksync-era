use crate::glue::GlueFrom;

impl GlueFrom<vm_m5::errors::VmRevertReason> for vm_vm1_3_2::errors::VmRevertReason {
    fn glue_from(value: vm_m5::errors::VmRevertReason) -> Self {
        match value {
            vm_m5::errors::VmRevertReason::General { msg } => Self::General {
                msg,
                data: Vec::new(),
            },
            vm_m5::errors::VmRevertReason::InnerTxError => Self::InnerTxError,
            vm_m5::errors::VmRevertReason::VmError => Self::VmError,
            vm_m5::errors::VmRevertReason::Unknown {
                function_selector,
                data,
            } => Self::Unknown {
                function_selector,
                data,
            },
        }
    }
}

impl GlueFrom<vm_m6::errors::VmRevertReason> for vm_vm1_3_2::errors::VmRevertReason {
    fn glue_from(value: vm_m6::errors::VmRevertReason) -> Self {
        match value {
            vm_m6::errors::VmRevertReason::General { msg, data } => Self::General { msg, data },
            vm_m6::errors::VmRevertReason::InnerTxError => Self::InnerTxError,
            vm_m6::errors::VmRevertReason::VmError => Self::VmError,
            vm_m6::errors::VmRevertReason::Unknown {
                function_selector,
                data,
            } => Self::Unknown {
                function_selector,
                data,
            },
        }
    }
}
