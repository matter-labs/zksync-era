#![deny(unreachable_pub)]
#![warn(unused_extern_crates)]
#![warn(unused_imports)]

pub use old_vm::{
    history_recorder::{HistoryDisabled, HistoryEnabled, HistoryMode},
    memory::SimpleMemory,
    oracles::storage::StorageOracle,
};

pub use vm_latest::{
    BytecodeCompressionError, Halt, TxRevertReason, VmRevertReason, VmRevertReasonParsingError,
};

pub use tracers::{
    call::CallTracer,
    traits::{BoxedTracer, DynTracer, ExecutionEndTracer, ExecutionProcessing, VmTracer},
    utils::VmExecutionStopReason,
    StorageInvocations, ValidationError, ValidationTracer, ValidationTracerParams,
};

pub use vm_latest::{
    BootloaderMemory, CurrentExecutionState, ExecutionResult, FinishedL1Batch, L1BatchEnv, L2Block,
    L2BlockEnv, Refunds, SystemEnv, TxExecutionMode, VmExecutionMode, VmExecutionResultAndLogs,
    VmExecutionStatistics,
};

pub use types::internals::ZkSyncVmState;

pub use utils::transaction_encoding::TransactionVmExt;

pub use bootloader_state::BootloaderState;

pub use crate::vm::Vm;

mod bootloader_state;
mod implementation;
mod old_vm;
mod tracers;
mod types;
mod vm;

pub mod constants;
pub mod utils;

#[cfg(test)]
mod tests;
