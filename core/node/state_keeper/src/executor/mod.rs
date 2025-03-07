use zksync_multivm::interface::{
    BatchTransactionExecutionResult, Call, ExecutionResult, Halt, VmExecutionMetrics,
    VmExecutionResultAndLogs,
};
pub use zksync_vm_executor::batch::MainBatchExecutorFactory;

#[cfg(test)]
mod tests;

/// State keeper representation of a transaction executed in the virtual machine.
///
/// A separate type allows to be more typesafe when dealing with halted transactions. It also simplifies testing seal criteria
/// (i.e., without picking transactions that actually produce appropriate `VmExecutionMetrics`).
#[derive(Debug, Clone)]
pub enum TxExecutionResult {
    /// Successful execution of the tx and the block tip dry run.
    Success {
        tx_result: Box<VmExecutionResultAndLogs>,
        tx_metrics: Box<VmExecutionMetrics>,
        call_tracer_result: Vec<Call>,
        gas_remaining: u32,
    },
    /// The VM rejected the tx for some reason.
    RejectedByVm { reason: Halt },
    /// Bootloader gas limit is not enough to execute the tx.
    BootloaderOutOfGasForTx,
}

impl TxExecutionResult {
    pub(crate) fn new(res: BatchTransactionExecutionResult) -> Self {
        match res.tx_result.result {
            ExecutionResult::Halt {
                reason: Halt::BootloaderOutOfGas,
            } => Self::BootloaderOutOfGasForTx,
            ExecutionResult::Halt { reason } => Self::RejectedByVm { reason },
            _ => Self::Success {
                tx_metrics: Box::new(res.tx_result.get_execution_metrics()),
                gas_remaining: res.tx_result.statistics.gas_remaining,
                tx_result: res.tx_result.clone(),
                call_tracer_result: res.call_traces,
            },
        }
    }

    /// Returns a revert reason if either transaction was rejected or bootloader ran out of gas.
    pub(super) fn err(&self) -> Option<&Halt> {
        match self {
            Self::Success { .. } => None,
            Self::RejectedByVm {
                reason: rejection_reason,
            } => Some(rejection_reason),
            Self::BootloaderOutOfGasForTx => Some(&Halt::BootloaderOutOfGas),
        }
    }
}
