use crate::{
    interface::{storage::WriteStorage, VmInterface},
    vm_boojum_integration::{tracers::DefaultExecutionTracer, vm::Vm},
    HistoryMode,
};

impl<S: WriteStorage, H: HistoryMode> Vm<S, H> {
    pub(crate) fn calculate_computational_gas_used(
        &self,
        tracer: &DefaultExecutionTracer<S, H::VmBoojumIntegration>,
        gas_remaining_before: u32,
        spent_pubdata_counter_before: u32,
    ) -> u32 {
        let total_gas_used = gas_remaining_before
            .checked_sub(self.gas_remaining())
            .expect("underflow");
        let gas_used_on_pubdata =
            tracer.gas_spent_on_pubdata(&self.state.local_state) - spent_pubdata_counter_before;
        total_gas_used
            .checked_sub(gas_used_on_pubdata)
            .unwrap_or_else(|| {
                tracing::error!(
                    "Gas used on pubdata is greater than total gas used. On pubdata: {}, total: {}",
                    gas_used_on_pubdata,
                    total_gas_used
                );
                0
            })
    }
}
