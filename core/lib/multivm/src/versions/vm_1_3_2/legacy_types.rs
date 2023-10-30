use zksync_types::{l2_to_l1_log::L2ToL1Log, StorageLogQuery, VmEvent};

///
/// Here we insert of the types that used to be present in lib/types at the time when this VM was in use.
///

/// Events/storage logs/l2->l1 logs created within transaction execution.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VmExecutionLogs {
    pub storage_logs: Vec<StorageLogQuery>,
    pub events: Vec<VmEvent>,
    pub l2_to_l1_logs: Vec<L2ToL1Log>,
    // This field moved to statistics, but we need to keep it for backward compatibility
    pub total_log_queries_count: usize,
}
