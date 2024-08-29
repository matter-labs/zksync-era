use crate::{interface::storage::WriteStorage, vm_latest::vm::Vm, HistoryMode};

impl<S: WriteStorage, H: HistoryMode> Vm<S, H> {
    pub(crate) fn calculate_computational_gas_used(&self, gas_remaining_before: u32) -> u32 {
        // Starting from VM version 1.5.0 pubdata was implicitly charged from users' gasLimit instead of
        // explicitly reduced from the `gas` in the VM state
        gas_remaining_before
            .checked_sub(self.gas_remaining())
            .expect("underflow")
    }
}
