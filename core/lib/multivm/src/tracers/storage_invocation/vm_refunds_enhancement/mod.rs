use crate::interface::tracer::{TracerExecutionStatus, TracerExecutionStopReason};
use crate::interface::{traits::tracers::dyn_tracers::vm_1_3_3::DynTracer, Halt};
use crate::tracers::storage_invocation::StorageInvocations;
use crate::vm_refunds_enhancement::VmTracer;
use crate::vm_refunds_enhancement::{BootloaderState, HistoryMode, SimpleMemory, ZkSyncVmState};
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
