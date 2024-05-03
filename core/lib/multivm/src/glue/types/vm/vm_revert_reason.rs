use crate::glue::GlueFrom;

impl GlueFrom<crate::vm_m5::errors::VmRevertReason> for crate::interface::VmRevertReason {
    fn glue_from(value: crate::vm_m5::errors::VmRevertReason) -> Self {
        match value {
            crate::vm_m5::errors::VmRevertReason::General { msg } => Self::General {
                msg,
                data: Vec::new(),
            },
            crate::vm_m5::errors::VmRevertReason::InnerTxError => Self::InnerTxError,
            crate::vm_m5::errors::VmRevertReason::VmError => Self::VmError,
            crate::vm_m5::errors::VmRevertReason::Unknown {
                function_selector,
                data,
            } => Self::Unknown {
                function_selector,
                data,
            },
        }
    }
}

impl GlueFrom<crate::vm_m6::errors::VmRevertReason> for crate::interface::VmRevertReason {
    fn glue_from(value: crate::vm_m6::errors::VmRevertReason) -> Self {
        match value {
            crate::vm_m6::errors::VmRevertReason::General { msg, data } => {
                Self::General { msg, data }
            }
            crate::vm_m6::errors::VmRevertReason::InnerTxError => Self::InnerTxError,
            crate::vm_m6::errors::VmRevertReason::VmError => Self::VmError,
            crate::vm_m6::errors::VmRevertReason::Unknown {
                function_selector,
                data,
            } => Self::Unknown {
                function_selector,
                data,
            },
        }
    }
}

impl GlueFrom<crate::vm_1_3_2::errors::VmRevertReason> for crate::interface::VmRevertReason {
    fn glue_from(value: crate::vm_1_3_2::errors::VmRevertReason) -> Self {
        match value {
            crate::vm_1_3_2::errors::VmRevertReason::General { msg, data } => {
                Self::General { msg, data }
            }
            crate::vm_1_3_2::errors::VmRevertReason::InnerTxError => Self::InnerTxError,
            crate::vm_1_3_2::errors::VmRevertReason::VmError => Self::VmError,
            crate::vm_1_3_2::errors::VmRevertReason::Unknown {
                function_selector,
                data,
            } => Self::Unknown {
                function_selector,
                data,
            },
        }
    }
}
