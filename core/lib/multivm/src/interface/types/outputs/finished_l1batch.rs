use super::{BootloaderMemory, CurrentExecutionState, VmExecutionResultAndLogs};

/// State of the VM after the batch execution.
#[derive(Debug, Clone)]
pub struct FinishedL1Batch {
    /// Result of the execution of the block tip part of the batch.
    pub block_tip_execution_result: VmExecutionResultAndLogs,
    /// State of the VM after the execution of the last transaction.
    pub final_execution_state: CurrentExecutionState,
    /// Memory of the bootloader with all executed transactions. Could be optional for old versions of the VM.
    pub final_bootloader_memory: Option<BootloaderMemory>,
}
