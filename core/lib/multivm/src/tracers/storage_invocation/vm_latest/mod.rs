use zksync_types::StopToken;

use crate::{
    interface::{
        storage::WriteStorage,
        tracer::{TracerExecutionStatus, TracerExecutionStopReason},
        Halt,
    },
    tracers::{dynamic::vm_1_5_2::DynTracer, StorageInvocations},
    vm_latest::{BootloaderState, HistoryMode, SimpleMemory, VmTracer, ZkSyncVmState},
};

impl<S, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for StorageInvocations {}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for StorageInvocations {
    fn finish_cycle(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        if self.stop_token.as_ref().is_some_and(StopToken::should_stop) {
            return TracerExecutionStatus::Stop(TracerExecutionStopReason::Abort(
                Halt::TracerCustom("Requested to stop".to_string()),
            ));
        }

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
