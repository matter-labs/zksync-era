use zk_evm_1_3_3::aux_structures::LogQuery;
use zksync_types::StorageLogKind;

/// Log query, which handle initial and repeated writes to the storage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StorageLogQuery {
    pub log_query: LogQuery,
    pub log_type: StorageLogKind,
}
