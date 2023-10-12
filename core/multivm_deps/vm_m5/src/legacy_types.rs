use zksync_types::{
    l2_to_l1_log::L2ToL1Log, LogQuery, StorageLogQuery, StorageLogQueryType, VmEvent,
};

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

// /// Log query, which handle initial and repeated writes to the storage
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub struct StorageLogQuery {
//     pub log_query: LogQuery,
//     pub log_type: StorageLogQueryType,
// }
