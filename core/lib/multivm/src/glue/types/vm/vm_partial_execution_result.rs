use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<vm_m5::vm::VmPartialExecutionResult> for vm_vm1_3_2::vm::VmPartialExecutionResult {
    fn glue_from(value: vm_m5::vm::VmPartialExecutionResult) -> Self {
        Self {
            logs: value.logs.glue_into(),
            revert_reason: value.revert_reason.map(GlueInto::glue_into),
            contracts_used: value.contracts_used,
            cycles_used: value.cycles_used,
            computational_gas_used: 0,
        }
    }
}

impl GlueFrom<vm_m6::vm::VmPartialExecutionResult> for vm_vm1_3_2::vm::VmPartialExecutionResult {
    fn glue_from(value: vm_m6::vm::VmPartialExecutionResult) -> Self {
        Self {
            logs: value.logs.glue_into(),
            revert_reason: value.revert_reason.map(GlueInto::glue_into),
            contracts_used: value.contracts_used,
            cycles_used: value.cycles_used,
            computational_gas_used: 0,
        }
    }
}
