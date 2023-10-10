use crate::glue::GlueFrom;
use vm_latest::VmExecutionStatistics;

impl GlueFrom<vm_virtual_blocks::VmExecutionStatistics> for VmExecutionStatistics {
    fn glue_from(value: vm_virtual_blocks::VmExecutionStatistics) -> Self {
        Self {
            contracts_used: value.contracts_used,
            cycles_used: value.cycles_used,
            gas_used: value.gas_used,
            computational_gas_used: value.computational_gas_used,
            total_log_queries: value.total_log_queries,
        }
    }
}
