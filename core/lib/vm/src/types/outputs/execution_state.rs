use zksync_types::l2_to_l1_log::L2ToL1Log;
use zksync_types::{LogQuery, StorageLogQuery, VmEvent, U256};

/// State of the VM since the start of the batch execution.
#[derive(Debug, Clone, PartialEq)]
pub struct CurrentExecutionState {
    /// Events produced by the VM.
    pub events: Vec<VmEvent>,
    /// Storage logs produced by the VM.
    pub storage_log_queries: Vec<StorageLogQuery>,
    /// Hashes of the contracts used by the VM.
    pub used_contract_hashes: Vec<U256>,
    /// L2 to L1 logs produced by the VM.
    pub system_logs: Vec<L2ToL1Log>,
    /// L2 to L1 logs produced by the L1Messenger
    pub user_l2_to_l1_logs: Vec<L2ToL1Log>,
    /// Number of log queries produced by the VM. Including l2_to_l1 logs, storage logs and events.
    pub total_log_queries: usize,
    /// Number of cycles used by the VM.
    pub cycles_used: u32,
    /// Sorted & deduplicated events logs for batch
    pub deduplicated_events_logs: Vec<LogQuery>,
}

/// Bootloader Memory of the VM.
pub type BootloaderMemory = Vec<(usize, U256)>;
