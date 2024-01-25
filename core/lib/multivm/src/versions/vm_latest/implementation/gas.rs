use zksync_state::WriteStorage;

use crate::{
    vm_latest::{tracers::DefaultExecutionTracer, vm::Vm},
    HistoryMode,
};

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
        tracer: &DefaultExecutionTracer<S, H::Vm1_4_1>,
        gas_remaining_before: u32,
        spent_pubdata_counter_before: u32,
    ) -> u32 {
        // Starting from VM version 1.5.0 pubdata was implicitly charged from users' gasLimit instead of
        // explicitly reduced from the `gas` in the VM state
        gas_remaining_before
            .checked_sub(self.gas_remaining())
            .expect("underflow")
    }
}
