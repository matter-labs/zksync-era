use crate::bootloader_state::BootloaderState;
use crate::old_vm::history_recorder::HistoryMode;
use crate::tracers::traits::{
    DynTracer, ExecutionProcessing, TracerExecutionStatus, TracerExecutionStopReason, VmTracer,
};
use crate::types::internals::ZkSyncVmState;
use crate::Halt;
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
impl<S, H: HistoryMode> DynTracer<S, H> for StorageInvocations {}

impl<S: WriteStorage, H: HistoryMode> ExecutionProcessing<S, H> for StorageInvocations {
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

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for StorageInvocations {}
