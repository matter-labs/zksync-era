use zksync_vm_interface::storage::WriteStorage;

use crate::vm_latest::{HistoryMode, ZkSyncVmState};

#[derive(Debug, Clone)]
pub(crate) struct GasLimiter {
    withheld_gas: WithheldGas,
    remaining_gas_limit: u32,
}

#[derive(Debug, Clone)]
enum WithheldGas {
    None,
    Pending,
    Withholding { withheld: u32, provided: u32 },
    PendingRelease { withheld: u32, provided: u32 },
}

impl GasLimiter {
    pub fn new(gas_limit: u32) -> Self {
        Self {
            withheld_gas: WithheldGas::None,
            remaining_gas_limit: gas_limit,
        }
    }

    pub fn start_limiting(&mut self) {
        assert!(matches!(self.withheld_gas, WithheldGas::None));
        self.withheld_gas = WithheldGas::Pending;
    }

    pub fn stop_limiting(&mut self) {
        if let WithheldGas::Withholding { withheld, provided } = self.withheld_gas {
            self.withheld_gas = WithheldGas::PendingRelease { withheld, provided };
        } else {
            panic!("Must enter account validation before exiting it");
        }
    }

    pub fn finish_cycle<S: WriteStorage, H: HistoryMode>(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
    ) {
        match self.withheld_gas {
            WithheldGas::Pending => {
                let to_remove = state
                    .local_state
                    .callstack
                    .current
                    .ergs_remaining
                    .saturating_sub(self.remaining_gas_limit);
                state.local_state.callstack.current.ergs_remaining -= to_remove;
                self.withheld_gas = WithheldGas::Withholding {
                    withheld: to_remove,
                    provided: state.local_state.callstack.current.ergs_remaining,
                };
            }
            WithheldGas::PendingRelease { withheld, provided } => {
                self.remaining_gas_limit -=
                    provided - state.local_state.callstack.current.ergs_remaining;
                state.local_state.callstack.current.ergs_remaining += withheld;
                self.withheld_gas = WithheldGas::None;
            }
            _ => {}
        }
    }
}
