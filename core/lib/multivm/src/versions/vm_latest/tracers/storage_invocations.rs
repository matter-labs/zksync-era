use crate::interface::traits::tracers::dyn_tracers::vm_1_3_3::DynTracer;
use crate::interface::Halt;
use crate::vm_latest::bootloader_state::BootloaderState;
use crate::vm_latest::old_vm::history_recorder::HistoryMode;
use crate::vm_latest::tracers::traits::{
    TracerExecutionStatus, TracerExecutionStopReason, VmTracer,
};
use crate::vm_latest::types::internals::ZkSyncVmState;
use crate::vm_latest::SimpleMemory;
use zksync_state::WriteStorage;

#[derive(Debug, Default, Clone)]
pub struct StorageInvocations {
    pub limit: usize,
}

impl StorageInvocations {
    pub fn new(limit: usize) -> Self {
        Self { limit }
    }
}

/// Tracer responsible for calculating the number of storage invocations and
/// stopping the VM execution if the limit is reached.
impl<S, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for StorageInvocations {}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for StorageInvocations {
    fn finish_cycle(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        let current = state
            .storage
            .storage
            .get_ptr()
            .borrow()
            .missed_storage_invocations();

        if current >= self.limit {
            return TracerExecutionStatus::Stop(TracerExecutionStopReason::Abort(
                Halt::TracerCustom("Storage invocations limit reached".to_string()),
            ));
        }
        TracerExecutionStatus::Continue
    }
}
