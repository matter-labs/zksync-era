#![deny(unreachable_pub)]
#![deny(unused_crate_dependencies)]
#![warn(unused_extern_crates)]

pub use old_vm::{
    history_recorder::{HistoryDisabled, HistoryEnabled, HistoryMode},
    memory::SimpleMemory,
    oracles::storage::StorageOracle,
};

pub use errors::{
    BytecodeCompressionError, Halt, TxRevertReason, VmRevertReason, VmRevertReasonParsingError,
};

pub use tracers::{
    call::CallTracer,
    traits::{BoxedTracer, DynTracer, ExecutionEndTracer, ExecutionProcessing, VmTracer},
    utils::VmExecutionStopReason,
    validation::ViolatedValidationRule,
    StorageInvocations, ValidationError, ValidationTracer, ValidationTracerParams,
};

pub use types::{
    inputs::{L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode, VmExecutionMode},
    internals::ZkSyncVmState,
    outputs::{
        BootloaderMemory, CurrentExecutionState, ExecutionResult, FinishedL1Batch, L2Block,
        Refunds, VmExecutionLogs, VmExecutionResultAndLogs, VmExecutionStatistics, VmMemoryMetrics,
    },
};
pub use utils::transaction_encoding::TransactionVmExt;

pub use bootloader_state::BootloaderState;

pub use crate::vm::Vm;

mod bootloader_state;
mod errors;
mod implementation;
mod old_vm;
mod tracers;
mod types;
mod vm;

pub mod constants;
pub mod utils;

#[cfg(test)]
mod tests;
