use crate::glue::{GlueFrom, GlueInto};
use vm_latest::{ExecutionResult, Refunds, TxRevertReason, VmExecutionResultAndLogs};
use zksync_types::tx::tx_execution_info::TxExecutionStatus;

impl GlueFrom<vm_m5::vm::VmTxExecutionResult> for VmExecutionResultAndLogs {
    fn glue_from(value: vm_m5::vm::VmTxExecutionResult) -> Self {
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

impl GlueFrom<vm_m6::vm::VmTxExecutionResult> for VmExecutionResultAndLogs {
    fn glue_from(value: vm_m6::vm::VmTxExecutionResult) -> Self {
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

impl GlueFrom<vm_1_3_2::vm::VmTxExecutionResult> for VmExecutionResultAndLogs {
    fn glue_from(value: vm_1_3_2::vm::VmTxExecutionResult) -> Self {
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

impl GlueFrom<Result<vm_m6::vm::VmTxExecutionResult, vm_m6::TxRevertReason>>
    for VmExecutionResultAndLogs
{
    fn glue_from(value: Result<vm_m6::vm::VmTxExecutionResult, vm_m6::TxRevertReason>) -> Self {
        match value {
            Ok(result) => result.glue_into(),
            Err(err) => {
                let revert: vm_latest::TxRevertReason = err.glue_into();
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

impl GlueFrom<Result<vm_1_3_2::vm::VmTxExecutionResult, vm_1_3_2::TxRevertReason>>
    for VmExecutionResultAndLogs
{
    fn glue_from(
        value: Result<vm_1_3_2::vm::VmTxExecutionResult, vm_1_3_2::TxRevertReason>,
    ) -> Self {
        match value {
            Ok(result) => result.glue_into(),
            Err(err) => {
                let revert: vm_latest::TxRevertReason = err.glue_into();
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

impl GlueFrom<Result<vm_m5::vm::VmTxExecutionResult, vm_m5::TxRevertReason>>
    for VmExecutionResultAndLogs
{
    fn glue_from(value: Result<vm_m5::vm::VmTxExecutionResult, vm_m5::TxRevertReason>) -> Self {
        match value {
            Ok(result) => result.glue_into(),
            Err(err) => {
                let revert: vm_latest::TxRevertReason = err.glue_into();
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
