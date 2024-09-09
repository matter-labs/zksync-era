use zksync_vm_interface::storage::WriteStorage;

use crate::vm_latest::{HistoryMode, ZkSyncVmState};

#[derive(Debug, Clone)]
pub(crate) struct GasLimiter {
    withheld_gas: WithheldGas,
}

#[derive(Debug, Clone)]
enum WithheldGas {
    None,
    Pending,
    Withholding { withheld: u32, provided: u32 },
    PendingRelease { withheld: u32, provided: u32 },
}

impl GasLimiter {
    pub fn new() -> Self {
        Self {
            withheld_gas: WithheldGas::None,
        }
    }

    pub fn start_limiting(&mut self) {
        assert!(matches!(self.withheld_gas, WithheldGas::None));
        self.withheld_gas = WithheldGas::Pending;
    }

    pub fn stop_limiting(&mut self) {
        if let WithheldGas::Withholding { withheld, provided } = self.withheld_gas {
            self.withheld_gas = WithheldGas::PendingRelease { withheld, provided };
        }
    }

    pub fn finish_cycle<S: WriteStorage, H: HistoryMode>(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        gas_limit: u32,
        gas_used: &mut u32,
    ) {
        match self.withheld_gas {
            WithheldGas::Pending => {
                let to_remove = state
                    .local_state
                    .callstack
                    .current
                    .ergs_remaining
                    .saturating_sub(gas_limit - *gas_used);
                state.local_state.callstack.current.ergs_remaining -= to_remove;
                self.withheld_gas = WithheldGas::Withholding {
                    withheld: to_remove,
                    provided: state.local_state.callstack.current.ergs_remaining,
                };
            }
            WithheldGas::PendingRelease { withheld, provided } => {
                *gas_used += provided - state.local_state.callstack.current.ergs_remaining;
                state.local_state.callstack.current.ergs_remaining += withheld;
                self.withheld_gas = WithheldGas::None;
            }
            _ => {}
        }
    }
}
