use zksync_types::writes::StateDiffRecord;

use super::{BootloaderMemory, CurrentExecutionState, VmExecutionResultAndLogs};

/// State of the VM after the batch execution.
#[derive(Debug, Clone)]
pub struct FinishedL1Batch {
    /// Result of the execution of the block tip part of the batch.
    pub block_tip_execution_result: VmExecutionResultAndLogs,
    /// State of the VM after the execution of the last transaction.
    pub final_execution_state: CurrentExecutionState,
    /// Memory of the bootloader with all executed transactions. Could be none for old versions of the VM.
    pub final_bootloader_memory: Option<BootloaderMemory>,
    /// Pubdata to be published on L1. Could be none for old versions of the VM.
    pub pubdata_input: Option<Vec<u8>>,
    /// List of state diffs. Could be none for old versions of the VM.
    pub state_diffs: Option<Vec<StateDiffRecord>>,
}
