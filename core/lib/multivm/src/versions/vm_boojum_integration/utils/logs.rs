use zk_evm_1_3_3::aux_structures::LogQuery;
use zk_evm_1_4_0::aux_structures::Timestamp;
use zksync_state::WriteStorage;
use zksync_types::{l2_to_l1_log::L2ToL1Log, StorageLogQueryType, VmEvent};

use crate::{
    glue::GlueInto,
    interface::L1BatchEnv,
    vm_boojum_integration::{
        old_vm::{events::merge_events, history_recorder::HistoryMode},
        types::internals::ZkSyncVmState,
    },
};

pub(crate) fn collect_events_and_l1_system_logs_after_timestamp<S: WriteStorage, H: HistoryMode>(
    vm_state: &ZkSyncVmState<S, H>,
    batch_env: &L1BatchEnv,
    from_timestamp: Timestamp,
) -> (Vec<VmEvent>, Vec<L2ToL1Log>) {
    let (raw_events, l1_messages) = vm_state
        .event_sink
        .get_events_and_l2_l1_logs_after_timestamp(from_timestamp);
    let events = merge_events(raw_events)
        .into_iter()
        .map(|e| e.into_vm_event(batch_env.number))
        .collect();
    (
        events,
        l1_messages.into_iter().map(GlueInto::glue_into).collect(),
    )
}

/// Log query, which handle initial and repeated writes to the storage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StorageLogQuery {
    pub log_query: LogQuery,
    pub log_type: StorageLogQueryType,
}
