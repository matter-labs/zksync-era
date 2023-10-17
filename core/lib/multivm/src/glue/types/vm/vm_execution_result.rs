use crate::glue::GlueFrom;

impl GlueFrom<crate::vm_m5::vm::VmExecutionResult> for crate::vm_latest::CurrentExecutionState {
    fn glue_from(value: crate::vm_m5::vm::VmExecutionResult) -> Self {
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

impl GlueFrom<crate::vm_m6::vm::VmExecutionResult> for crate::vm_latest::CurrentExecutionState {
    fn glue_from(value: crate::vm_m6::vm::VmExecutionResult) -> Self {
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

impl GlueFrom<crate::vm_1_3_2::VmExecutionResult> for crate::vm_latest::CurrentExecutionState {
    fn glue_from(value: crate::vm_1_3_2::VmExecutionResult) -> Self {
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
