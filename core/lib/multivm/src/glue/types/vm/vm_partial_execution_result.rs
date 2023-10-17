use crate::glue::{GlueFrom, GlueInto};
use zksync_types::tx::tx_execution_info::VmExecutionLogs;

impl GlueFrom<crate::vm_m5::vm::VmPartialExecutionResult>
    for crate::vm_latest::VmExecutionResultAndLogs
{
    fn glue_from(value: crate::vm_m5::vm::VmPartialExecutionResult) -> Self {
        Self {
            result: value.revert_reason.glue_into(),
            logs: VmExecutionLogs {
                events: value.logs.events.clone(),
                l2_to_l1_logs: value.logs.l2_to_l1_logs.clone(),
                storage_logs: value.logs.storage_logs.clone(),
                total_log_queries_count: value.logs.total_log_queries_count,
            },
            statistics: crate::vm_latest::VmExecutionStatistics {
                contracts_used: value.contracts_used,
                cycles_used: value.cycles_used,
                total_log_queries: value.logs.total_log_queries_count,
                // There are no such fields in m5
                gas_used: 0,
                // There are no such fields in m5
                computational_gas_used: 0,
            },
            refunds: crate::vm_latest::Refunds {
                gas_refunded: 0,
                operator_suggested_refund: 0,
            },
        }
    }
}

impl GlueFrom<crate::vm_m6::vm::VmPartialExecutionResult>
    for crate::vm_latest::VmExecutionResultAndLogs
{
    fn glue_from(value: crate::vm_m6::vm::VmPartialExecutionResult) -> Self {
        Self {
            result: value.revert_reason.glue_into(),
            logs: VmExecutionLogs {
                events: value.logs.events.clone(),
                l2_to_l1_logs: value.logs.l2_to_l1_logs.clone(),
                storage_logs: value.logs.storage_logs.clone(),
                total_log_queries_count: value.logs.total_log_queries_count,
            },
            statistics: crate::vm_latest::VmExecutionStatistics {
                contracts_used: value.contracts_used,
                cycles_used: value.cycles_used,
                gas_used: value.computational_gas_used,
                computational_gas_used: value.computational_gas_used,
                total_log_queries: value.logs.total_log_queries_count,
            },
            refunds: crate::vm_latest::Refunds {
                gas_refunded: 0,
                operator_suggested_refund: 0,
            },
        }
    }
}

impl GlueFrom<crate::vm_1_3_2::vm::VmPartialExecutionResult>
    for crate::vm_latest::VmExecutionResultAndLogs
{
    fn glue_from(value: crate::vm_1_3_2::vm::VmPartialExecutionResult) -> Self {
        Self {
            result: value.revert_reason.glue_into(),
            logs: VmExecutionLogs {
                events: value.logs.events.clone(),
                l2_to_l1_logs: value.logs.l2_to_l1_logs.clone(),
                storage_logs: value.logs.storage_logs.clone(),
                total_log_queries_count: value.logs.total_log_queries_count,
            },
            statistics: crate::vm_latest::VmExecutionStatistics {
                contracts_used: value.contracts_used,
                cycles_used: value.cycles_used,
                gas_used: value.computational_gas_used,
                computational_gas_used: value.computational_gas_used,
                total_log_queries: value.logs.total_log_queries_count,
            },
            refunds: crate::vm_latest::Refunds {
                gas_refunded: 0,
                operator_suggested_refund: 0,
            },
        }
    }
}

impl GlueFrom<Option<crate::vm_m5::TxRevertReason>> for crate::vm_latest::ExecutionResult {
    fn glue_from(value: Option<crate::vm_m5::TxRevertReason>) -> Self {
        if let Some(error) = value {
            let error_reason: crate::vm_latest::TxRevertReason = error.glue_into();
            match error_reason {
                crate::vm_latest::TxRevertReason::TxReverted(reason) => {
                    Self::Revert { output: reason }
                }
                crate::vm_latest::TxRevertReason::Halt(halt) => Self::Halt { reason: halt },
            }
        } else {
            Self::Success { output: vec![] }
        }
    }
}

impl GlueFrom<Option<crate::vm_m6::TxRevertReason>> for crate::vm_latest::ExecutionResult {
    fn glue_from(value: Option<crate::vm_m6::TxRevertReason>) -> Self {
        if let Some(error) = value {
            let error_reason: crate::vm_latest::TxRevertReason = error.glue_into();
            match error_reason {
                crate::vm_latest::TxRevertReason::TxReverted(reason) => {
                    Self::Revert { output: reason }
                }
                crate::vm_latest::TxRevertReason::Halt(halt) => Self::Halt { reason: halt },
            }
        } else {
            Self::Success { output: vec![] }
        }
    }
}

impl GlueFrom<Option<crate::vm_1_3_2::TxRevertReason>> for crate::vm_latest::ExecutionResult {
    fn glue_from(value: Option<crate::vm_1_3_2::TxRevertReason>) -> Self {
        if let Some(error) = value {
            let error_reason: crate::vm_latest::TxRevertReason = error.glue_into();
            match error_reason {
                crate::vm_latest::TxRevertReason::TxReverted(reason) => {
                    Self::Revert { output: reason }
                }
                crate::vm_latest::TxRevertReason::Halt(halt) => Self::Halt { reason: halt },
            }
        } else {
            Self::Success { output: vec![] }
        }
    }
}
