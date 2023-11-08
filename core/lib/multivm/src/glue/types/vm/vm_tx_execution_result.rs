use crate::glue::{GlueFrom, GlueInto};
use crate::interface::{ExecutionResult, Refunds, TxRevertReason, VmExecutionResultAndLogs};
use zksync_types::tx::tx_execution_info::TxExecutionStatus;

impl GlueFrom<crate::vm_m5::vm_instance::VmTxExecutionResult> for VmExecutionResultAndLogs {
    fn glue_from(value: crate::vm_m5::vm_instance::VmTxExecutionResult) -> Self {
        let mut result: VmExecutionResultAndLogs = value.result.glue_into();
        if result.result.is_failed() {
            assert_eq!(value.status, TxExecutionStatus::Failure);
        }

        result.refunds = Refunds {
            gas_refunded: value.gas_refunded,
            operator_suggested_refund: value.operator_suggested_refund,
        };
        result
    }
}

impl GlueFrom<crate::vm_m6::vm_instance::VmTxExecutionResult> for VmExecutionResultAndLogs {
    fn glue_from(value: crate::vm_m6::vm_instance::VmTxExecutionResult) -> Self {
        let mut result: VmExecutionResultAndLogs = value.result.glue_into();
        if result.result.is_failed() {
            assert_eq!(value.status, TxExecutionStatus::Failure);
        }

        result.refunds = Refunds {
            gas_refunded: value.gas_refunded,
            operator_suggested_refund: value.operator_suggested_refund,
        };
        result
    }
}

impl GlueFrom<crate::vm_1_3_2::vm_instance::VmTxExecutionResult> for VmExecutionResultAndLogs {
    fn glue_from(value: crate::vm_1_3_2::vm_instance::VmTxExecutionResult) -> Self {
        let mut result: VmExecutionResultAndLogs = value.result.glue_into();
        if result.result.is_failed() {
            assert_eq!(value.status, TxExecutionStatus::Failure);
        }

        result.refunds = Refunds {
            gas_refunded: value.gas_refunded,
            operator_suggested_refund: value.operator_suggested_refund,
        };
        result
    }
}

impl GlueFrom<Result<crate::vm_m6::vm_instance::VmTxExecutionResult, crate::vm_m6::TxRevertReason>>
    for VmExecutionResultAndLogs
{
    fn glue_from(
        value: Result<crate::vm_m6::vm_instance::VmTxExecutionResult, crate::vm_m6::TxRevertReason>,
    ) -> Self {
        match value {
            Ok(result) => result.glue_into(),
            Err(err) => {
                let revert: crate::interface::TxRevertReason = err.glue_into();
                match revert {
                    TxRevertReason::TxReverted(err) => VmExecutionResultAndLogs {
                        result: ExecutionResult::Revert { output: err },
                        logs: Default::default(),
                        statistics: Default::default(),
                        refunds: Default::default(),
                    },
                    TxRevertReason::Halt(halt) => VmExecutionResultAndLogs {
                        result: ExecutionResult::Halt { reason: halt },
                        logs: Default::default(),
                        statistics: Default::default(),
                        refunds: Default::default(),
                    },
                }
            }
        }
    }
}

impl
    GlueFrom<
        Result<crate::vm_1_3_2::vm_instance::VmTxExecutionResult, crate::vm_1_3_2::TxRevertReason>,
    > for VmExecutionResultAndLogs
{
    fn glue_from(
        value: Result<
            crate::vm_1_3_2::vm_instance::VmTxExecutionResult,
            crate::vm_1_3_2::TxRevertReason,
        >,
    ) -> Self {
        match value {
            Ok(result) => result.glue_into(),
            Err(err) => {
                let revert: crate::interface::TxRevertReason = err.glue_into();
                match revert {
                    TxRevertReason::TxReverted(err) => VmExecutionResultAndLogs {
                        result: ExecutionResult::Revert { output: err },
                        logs: Default::default(),
                        statistics: Default::default(),
                        refunds: Default::default(),
                    },
                    TxRevertReason::Halt(halt) => VmExecutionResultAndLogs {
                        result: ExecutionResult::Halt { reason: halt },
                        logs: Default::default(),
                        statistics: Default::default(),
                        refunds: Default::default(),
                    },
                }
            }
        }
    }
}

impl GlueFrom<Result<crate::vm_m5::vm_instance::VmTxExecutionResult, crate::vm_m5::TxRevertReason>>
    for VmExecutionResultAndLogs
{
    fn glue_from(
        value: Result<crate::vm_m5::vm_instance::VmTxExecutionResult, crate::vm_m5::TxRevertReason>,
    ) -> Self {
        match value {
            Ok(result) => result.glue_into(),
            Err(err) => {
                let revert: crate::interface::TxRevertReason = err.glue_into();
                match revert {
                    TxRevertReason::Halt(halt) => VmExecutionResultAndLogs {
                        result: ExecutionResult::Halt { reason: halt },
                        logs: Default::default(),
                        statistics: Default::default(),
                        refunds: Default::default(),
                    },
                    _ => {
                        unreachable!("Halt is the only revert reason for VM 5")
                    }
                }
            }
        }
    }
}
