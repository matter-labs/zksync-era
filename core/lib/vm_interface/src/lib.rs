//! ZKsync Era VM interfaces.

pub use crate::{
    types::{
        errors::{
            BytecodeCompressionError, Halt, TxRevertReason, VmRevertReason,
            VmRevertReasonParsingError,
        },
        inputs::{L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode, VmExecutionMode},
        outputs::{
            compress_bytecode, BootloaderMemory, CompressedBytecodeInfo, CurrentExecutionState,
            ExecutionResult, FailedToCompressBytecodeError, FinishedL1Batch, L2Block, Refunds,
            TransactionExecutionResult, TxExecutionStatus, VmExecutionLogs,
            VmExecutionResultAndLogs, VmExecutionStatistics, VmMemoryMetrics,
        },
        tracer,
    },
    vm::{VmFactory, VmInterface, VmInterfaceHistoryEnabled},
};

pub mod storage;
mod types;
mod vm;
