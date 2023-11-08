use crate::interface::dyn_tracers::vm_1_3_3::DynTracer;
use crate::tracers::storage_invocation::StorageInvocations;
use crate::vm_virtual_blocks::{
    BootloaderState, ExecutionEndTracer, ExecutionProcessing, HistoryMode, SimpleMemory, VmTracer,
    ZkSyncVmState,
};
use zksync_state::WriteStorage;

impl<H: HistoryMode> ExecutionEndTracer<H> for StorageInvocations {
    fn should_stop_execution(&self) -> bool {
        self.current >= self.limit
    }
}

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for StorageInvocations {}

impl<S: WriteStorage, H: HistoryMode> ExecutionProcessing<S, H> for StorageInvocations {
    fn after_cycle(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &mut BootloaderState,
    ) {
        self.current = state
            .storage
            .storage
            .get_ptr()
            .borrow()
            .missed_storage_invocations();
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for StorageInvocations {}
