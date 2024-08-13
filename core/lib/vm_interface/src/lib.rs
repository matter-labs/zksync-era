//! ZKsync Era VM interfaces.

pub use crate::{
    types::{
        errors::{
            BytecodeCompressionError, Halt, TxRevertReason, VmRevertReason,
            VmRevertReasonParsingError,
        },
        inputs::{L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode, VmExecutionMode},
        outputs::{
            BootloaderMemory, CurrentExecutionState, ExecutionResult, FinishedL1Batch, L2Block,
            Refunds, VmExecutionLogs, VmExecutionResultAndLogs, VmExecutionStatistics,
            VmMemoryMetrics,
        },
        tracer,
    },
    vm::{VmFactory, VmInterface, VmInterfaceHistoryEnabled},
};

pub mod storage;
mod types;
mod vm;
