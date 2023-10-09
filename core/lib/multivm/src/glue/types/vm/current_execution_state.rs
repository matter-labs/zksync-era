use crate::glue::GlueFrom;
use vm_latest::CurrentExecutionState;

impl GlueFrom<vm_virtual_blocks::CurrentExecutionState> for CurrentExecutionState {
    fn glue_from(value: vm_virtual_blocks::CurrentExecutionState) -> Self {
        Self {
            events: value.events,
            storage_log_queries: value.storage_log_queries,
            used_contract_hashes: value.used_contract_hashes,
            l2_to_l1_logs: value.l2_to_l1_logs,
            total_log_queries: value.total_log_queries,
            cycles_used: value.cycles_used,
        }
    }
}
