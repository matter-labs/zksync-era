//! ZKsync Era VM interfaces.

pub use crate::{
    types::{
        errors::{
            BytecodeCompressionError, Halt, TxRevertReason, VmRevertReason,
            VmRevertReasonParsingError,
        },
        inputs::{L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode, VmExecutionMode},
        outputs::{
            BootloaderMemory, CompressedBytecodeInfo, CurrentExecutionState,
            DeduplicatedWritesMetrics, ExecutionMetrics, ExecutionResult, FinishedL1Batch, L2Block,
            Refunds, TransactionExecutionResult, TxExecutionStatus, VmExecutionLogs,
            VmExecutionResultAndLogs, VmExecutionStatistics, VmMemoryMetrics,
        },
        tracer,
    },
    vm::{VmFactory, VmInterface, VmInterfaceHistoryEnabled},
};

pub mod storage;
mod types;
mod vm;
