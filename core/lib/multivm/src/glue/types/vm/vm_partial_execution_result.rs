use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<crate::vm_m5::vm_instance::VmPartialExecutionResult>
    for crate::interface::VmExecutionResultAndLogs
{
    fn glue_from(value: crate::vm_m5::vm_instance::VmPartialExecutionResult) -> Self {
        Self {
            result: value.revert_reason.glue_into(),
            logs: value.logs.clone(),
            statistics: crate::interface::VmExecutionStatistics {
                contracts_used: value.contracts_used,
                cycles_used: value.cycles_used,
                total_log_queries: value.logs.total_log_queries_count,
                // There are no such fields in m5
                gas_used: 0,
                // There are no such fields in m5
                computational_gas_used: 0,
                pubdata_published: 0,
            },
            refunds: crate::interface::Refunds {
                gas_refunded: 0,
                operator_suggested_refund: 0,
            },
        }
    }
}

impl GlueFrom<crate::vm_m6::vm_instance::VmPartialExecutionResult>
    for crate::interface::VmExecutionResultAndLogs
{
    fn glue_from(value: crate::vm_m6::vm_instance::VmPartialExecutionResult) -> Self {
        Self {
            result: value.revert_reason.glue_into(),
            logs: value.logs.clone(),
            statistics: crate::interface::VmExecutionStatistics {
                contracts_used: value.contracts_used,
                cycles_used: value.cycles_used,
                gas_used: value.computational_gas_used,
                computational_gas_used: value.computational_gas_used,
                total_log_queries: value.logs.total_log_queries_count,
                pubdata_published: 0,
            },
            refunds: crate::interface::Refunds {
                gas_refunded: 0,
                operator_suggested_refund: 0,
            },
        }
    }
}

impl GlueFrom<crate::vm_1_3_2::vm_instance::VmPartialExecutionResult>
    for crate::interface::VmExecutionResultAndLogs
{
    fn glue_from(value: crate::vm_1_3_2::vm_instance::VmPartialExecutionResult) -> Self {
        Self {
            result: value.revert_reason.glue_into(),
            logs: value.logs.clone(),
            statistics: crate::interface::VmExecutionStatistics {
                contracts_used: value.contracts_used,
                cycles_used: value.cycles_used,
                gas_used: value.computational_gas_used,
                computational_gas_used: value.computational_gas_used,
                total_log_queries: value.logs.total_log_queries_count,
                pubdata_published: 0,
            },
            refunds: crate::interface::Refunds {
                gas_refunded: 0,
                operator_suggested_refund: 0,
            },
        }
    }
}

impl GlueFrom<Option<crate::vm_m5::TxRevertReason>> for crate::interface::ExecutionResult {
    fn glue_from(value: Option<crate::vm_m5::TxRevertReason>) -> Self {
        if let Some(error) = value {
            let error_reason: crate::interface::TxRevertReason = error.glue_into();
            match error_reason {
                crate::interface::TxRevertReason::TxReverted(reason) => {
                    Self::Revert { output: reason }
                }
                crate::interface::TxRevertReason::Halt(halt) => Self::Halt { reason: halt },
            }
        } else {
            Self::Success { output: vec![] }
        }
    }
}

impl GlueFrom<Option<crate::vm_m6::TxRevertReason>> for crate::interface::ExecutionResult {
    fn glue_from(value: Option<crate::vm_m6::TxRevertReason>) -> Self {
        if let Some(error) = value {
            let error_reason: crate::interface::TxRevertReason = error.glue_into();
            match error_reason {
                crate::interface::TxRevertReason::TxReverted(reason) => {
                    Self::Revert { output: reason }
                }
                crate::interface::TxRevertReason::Halt(halt) => Self::Halt { reason: halt },
            }
        } else {
            Self::Success { output: vec![] }
        }
    }
}

impl GlueFrom<Option<crate::vm_1_3_2::TxRevertReason>> for crate::interface::ExecutionResult {
    fn glue_from(value: Option<crate::vm_1_3_2::TxRevertReason>) -> Self {
        if let Some(error) = value {
            let error_reason: crate::interface::TxRevertReason = error.glue_into();
            match error_reason {
                crate::interface::TxRevertReason::TxReverted(reason) => {
                    Self::Revert { output: reason }
                }
                crate::interface::TxRevertReason::Halt(halt) => Self::Halt { reason: halt },
            }
        } else {
            Self::Success { output: vec![] }
        }
    }
}
