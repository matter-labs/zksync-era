use crate::interface::{
    tracer::{TracerExecutionStatus, TracerExecutionStopReason},
    traits::tracers::dyn_tracers::vm_1_4_0::DynTracer,
    Halt,
};
use crate::tracers::storage_invocation::StorageInvocations;
use crate::vm_latest::{BootloaderState, HistoryMode, SimpleMemory, VmTracer, ZkSyncVmState};
use zksync_state::WriteStorage;

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
