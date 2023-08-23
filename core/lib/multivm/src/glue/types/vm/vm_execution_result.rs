use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<vm_m5::VmExecutionResult> for vm_vm1_3_2::VmExecutionResult {
    fn glue_from(value: vm_m5::VmExecutionResult) -> Self {
        Self {
            events: value.events.into_iter().map(GlueInto::glue_into).collect(),
            storage_log_queries: value
                .storage_log_queries
                .into_iter()
                .map(GlueInto::glue_into)
                .collect(),
            used_contract_hashes: value.used_contract_hashes,
            l2_to_l1_logs: value
                .l2_to_l1_logs
                .into_iter()
                .map(GlueInto::glue_into)
                .collect(),
            return_data: value.return_data,
            gas_used: value.gas_used,
            computational_gas_used: value.gas_used, // Substitute due to lack of such field
            contracts_used: value.contracts_used,
            revert_reason: value.revert_reason.map(GlueFrom::glue_from),
            trace: zksync_types::vm_trace::VmTrace::ExecutionTrace(value.trace.glue_into()),
            total_log_queries: value.total_log_queries,
            cycles_used: value.cycles_used,
        }
    }
}

impl GlueFrom<vm_m6::VmExecutionResult> for vm_vm1_3_2::VmExecutionResult {
    fn glue_from(value: vm_m6::VmExecutionResult) -> Self {
        Self {
            events: value.events.into_iter().map(GlueInto::glue_into).collect(),
            storage_log_queries: value
                .storage_log_queries
                .into_iter()
                .map(GlueInto::glue_into)
                .collect(),
            used_contract_hashes: value.used_contract_hashes,
            l2_to_l1_logs: value
                .l2_to_l1_logs
                .into_iter()
                .map(GlueInto::glue_into)
                .collect(),
            return_data: value.return_data,
            gas_used: value.gas_used,
            computational_gas_used: value.gas_used, // Substitute due to lack of such field
            contracts_used: value.contracts_used,
            revert_reason: value.revert_reason.map(GlueFrom::glue_from),
            trace: value.trace.glue_into(),
            total_log_queries: value.total_log_queries,
            cycles_used: value.cycles_used,
        }
    }
}
