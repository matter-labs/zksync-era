use std::borrow::Cow;

pub use self::{
    bytecode::CompressedBytecodeInfo,
    execution_result::{
        BatchTransactionExecutionResult, Call, CallType, ExecutionResult,
        OneshotTransactionExecutionResult, Refunds, TransactionExecutionResult, TxExecutionStatus,
        VmEvent, VmExecutionLogs, VmExecutionResultAndLogs,
    },
    execution_state::{BootloaderMemory, CurrentExecutionState},
    finished_l1batch::FinishedL1Batch,
    l2_block::L2Block,
    statistic::{
        CircuitStatistic, DeduplicatedWritesMetrics, TransactionExecutionMetrics,
        VmExecutionMetrics, VmExecutionStatistics, VmMemoryMetrics,
    },
};

mod bytecode;
mod execution_result;
mod execution_state;
mod finished_l1batch;
mod l2_block;
mod statistic;

/// Result of pushing a transaction to the VM state without executing it.
#[derive(Debug)]
pub struct PushTransactionResult<'a> {
    /// Compressed bytecodes for the transaction. If the VM doesn't support bytecode compression, returns
    /// an empty slice.
    ///
    /// Importantly, these bytecodes are not guaranteed to be published by the transaction;
    /// e.g., it may run out of gas during publication.
    pub compressed_bytecodes: Cow<'a, [CompressedBytecodeInfo]>,
}
