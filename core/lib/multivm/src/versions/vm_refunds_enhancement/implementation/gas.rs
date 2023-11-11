use crate::HistoryMode;
use zksync_state::WriteStorage;

use crate::vm_refunds_enhancement::tracers::DefaultExecutionTracer;
use crate::vm_refunds_enhancement::vm::Vm;

impl<S: WriteStorage, H: HistoryMode> Vm<S, H> {
    /// Returns the amount of gas remaining to the VM.
    /// Note that this *does not* correspond to the gas limit of a transaction.
    /// To calculate the amount of gas spent by transaction, you should call this method before and after
    /// the execution, and subtract these values.
    ///
    /// Note: this method should only be called when either transaction is fully completed or VM completed
    /// its execution. Remaining gas value is read from the current stack frame, so if you'll attempt to
    /// read it during the transaction execution, you may receive invalid value.
    pub(crate) fn gas_remaining(&self) -> u32 {
        self.state.local_state.callstack.current.ergs_remaining
    }

    pub(crate) fn calculate_computational_gas_used(
        &self,
        tracer: &DefaultExecutionTracer<S, H::VmVirtualBlocksRefundsEnhancement>,
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
