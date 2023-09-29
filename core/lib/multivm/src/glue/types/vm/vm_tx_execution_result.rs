use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<vm_m5::vm::VmTxExecutionResult> for vm_vm1_3_2::vm::VmTxExecutionResult {
    fn glue_from(value: vm_m5::vm::VmTxExecutionResult) -> Self {
        Self {
            status: value.status.glue_into(),
            result: value.result.glue_into(),
            call_traces: vec![], // Substitute due to lack of fields.
            gas_refunded: value.gas_refunded,
            operator_suggested_refund: value.operator_suggested_refund,
        }
    }
}

impl GlueFrom<vm_m6::vm::VmTxExecutionResult> for vm_vm1_3_2::vm::VmTxExecutionResult {
    fn glue_from(value: vm_m6::vm::VmTxExecutionResult) -> Self {
        Self {
            status: value.status.glue_into(),
            result: value.result.glue_into(),
            call_traces: vec![], // Substitute due to lack of fields.
            gas_refunded: value.gas_refunded,
            operator_suggested_refund: value.operator_suggested_refund,
        }
    }
}
