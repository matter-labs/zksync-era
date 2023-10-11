use crate::glue::GlueFrom;

impl GlueFrom<vm_m5::vm::VmExecutionResult> for vm_latest::CurrentExecutionState {
    fn glue_from(value: vm_m5::vm::VmExecutionResult) -> Self {
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

impl GlueFrom<vm_m6::vm::VmExecutionResult> for vm_latest::CurrentExecutionState {
    fn glue_from(value: vm_m6::vm::VmExecutionResult) -> Self {
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

impl GlueFrom<vm_1_3_2::VmExecutionResult> for vm_latest::CurrentExecutionState {
    fn glue_from(value: vm_1_3_2::VmExecutionResult) -> Self {
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
