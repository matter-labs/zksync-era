use crate::{
    interface::storage::WriteStorage,
    tracers::{dyn_tracers::vm_1_3_3::DynTracer, storage_invocation::StorageInvocations},
    vm_virtual_blocks::{
        BootloaderState, ExecutionEndTracer, ExecutionProcessing, HistoryMode, SimpleMemory,
        VmTracer, ZkSyncVmState,
    },
};

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
