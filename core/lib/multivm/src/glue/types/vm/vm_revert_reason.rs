use crate::glue::GlueFrom;

impl GlueFrom<vm_m5::errors::VmRevertReason> for vm_latest::VmRevertReason {
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

impl GlueFrom<vm_m6::errors::VmRevertReason> for vm_latest::VmRevertReason {
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

impl GlueFrom<vm_1_3_2::errors::VmRevertReason> for vm_latest::VmRevertReason {
    fn glue_from(value: vm_1_3_2::errors::VmRevertReason) -> Self {
        match value {
            vm_1_3_2::errors::VmRevertReason::General { msg, data } => Self::General { msg, data },
            vm_1_3_2::errors::VmRevertReason::InnerTxError => Self::InnerTxError,
            vm_1_3_2::errors::VmRevertReason::VmError => Self::VmError,
            vm_1_3_2::errors::VmRevertReason::Unknown {
                function_selector,
                data,
            } => Self::Unknown {
                function_selector,
                data,
            },
        }
    }
}

impl GlueFrom<vm_virtual_blocks::VmRevertReason> for vm_latest::VmRevertReason {
    fn glue_from(value: vm_virtual_blocks::VmRevertReason) -> Self {
        match value {
            vm_virtual_blocks::VmRevertReason::General { msg, data } => Self::General { msg, data },
            vm_virtual_blocks::VmRevertReason::InnerTxError => Self::InnerTxError,
            vm_virtual_blocks::VmRevertReason::VmError => Self::VmError,
            vm_virtual_blocks::VmRevertReason::Unknown {
                function_selector,
                data,
            } => Self::Unknown {
                function_selector,
                data,
            },
        }
    }
}
