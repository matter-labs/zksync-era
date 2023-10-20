use crate::interface::dyn_tracers::vm_1_3_3::DynTracer;
use crate::vm_virtual_blocks::bootloader_state::BootloaderState;
use crate::vm_virtual_blocks::old_vm::history_recorder::HistoryMode;
use crate::vm_virtual_blocks::tracers::traits::{
    ExecutionEndTracer, ExecutionProcessing, VmTracer,
};
use crate::vm_virtual_blocks::types::internals::ZkSyncVmState;
use crate::vm_virtual_blocks::SimpleMemory;
use zksync_state::WriteStorage;

#[derive(Debug, Default, Clone)]
pub struct StorageInvocations {
    limit: usize,
    current: usize,
}

impl StorageInvocations {
    pub fn new(limit: usize) -> Self {
        Self { limit, current: 0 }
    }
}

/// Tracer responsible for calculating the number of storage invocations and
/// stopping the VM execution if the limit is reached.
impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for StorageInvocations {}

impl<H: HistoryMode> ExecutionEndTracer<H> for StorageInvocations {
    fn should_stop_execution(&self) -> bool {
        self.current >= self.limit
    }
}

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
