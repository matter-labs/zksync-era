//! ZKsync Era VM interfaces.
//!
//! # Developer guidelines
//!
//! Which types should be put in this crate and which ones in `zksync_multivm` or other downstream crates?
//!
//! - This crate should contain logic not tied to a particular VM version; in contrast, most logic in `zksync_multivm`
//!   is version-specific.
//! - This crate should not have heavyweight dependencies (like VM implementations). Anything heavier than `serde` is discouraged.
//!   In contrast, `zksync_multivm` depends on old VM versions.
//! - If a type belongs in this crate, still be thorough about its methods. VM implementation details belong to `zksync_multivm`
//!   and should be implemented as functions / extension traits there, rather than as methods here.
//!
//! Which types should be put in this crate vs `zksync_types`?
//!
//! - In this case, we want to separate types by domain. If a certain type clearly belongs to the VM domain
//!   (e.g., can only be produced by VM execution), it probably belongs here. In contrast, if a type is more general / fundamental,
//!   it may belong to `zksync_types`.

pub use crate::{
    types::{
        errors::{
            BytecodeCompressionError, Halt, TxRevertReason, VmRevertReason,
            VmRevertReasonParsingError,
        },
        inputs::{
            L1BatchEnv, L2BlockEnv, OneshotEnv, StoredL2BlockEnv, SystemEnv, TxExecutionMode,
            VmExecutionMode,
        },
        outputs::{
            BootloaderMemory, Call, CallType, CircuitStatistic, CompressedBytecodeInfo,
            CurrentExecutionState, DeduplicatedWritesMetrics, ExecutionResult, FinishedL1Batch,
            L2Block, Refunds, TransactionExecutionMetrics, TransactionExecutionResult,
            TxExecutionStatus, VmExecutionLogs, VmExecutionMetrics, VmExecutionResultAndLogs,
            VmExecutionStatistics, VmMemoryMetrics,
        },
        tracer,
    },
    vm::{VmFactory, VmInterface, VmInterfaceHistoryEnabled},
};

pub mod storage;
mod types;
mod vm;
