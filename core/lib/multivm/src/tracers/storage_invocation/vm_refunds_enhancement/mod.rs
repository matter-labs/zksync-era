use crate::{
    interface::{
        storage::WriteStorage,
        tracer::{TracerExecutionStatus, TracerExecutionStopReason},
        Halt,
    },
    tracers::{dyn_tracers::vm_1_3_3::DynTracer, storage_invocation::StorageInvocations},
    vm_refunds_enhancement::{BootloaderState, HistoryMode, SimpleMemory, VmTracer, ZkSyncVmState},
};

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
