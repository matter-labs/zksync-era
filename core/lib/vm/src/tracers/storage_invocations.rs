use crate::bootloader_state::BootloaderState;
use crate::tracers::traits::{DynTracer, ExecutionEndTracer, ExecutionProcessing, VmTracer};
use crate::types::internals::ZkSyncVmState;
use zksync_state::WriteStorage;

#[derive(Debug, Default, Clone)]
pub struct StorageInvocations {
    pub limit: usize,
    current: usize,
}

impl StorageInvocations {
    pub fn new(limit: usize) -> Self {
        Self { limit, current: 0 }
    }
}

/// Tracer responsible for calculating the number of storage invocations and
/// stopping the VM execution if the limit is reached.
impl<S> DynTracer<S> for StorageInvocations {}

impl ExecutionEndTracer for StorageInvocations {
    fn should_stop_execution(&self) -> bool {
        self.current >= self.limit
    }
}

impl<S: WriteStorage> ExecutionProcessing<S> for StorageInvocations {
    fn after_cycle(
        &mut self,
        state: &mut ZkSyncVmState<S>,
        _bootloader_state: &mut BootloaderState,
    ) {
        self.current = state
            .storage
            .get_ptr()
            .borrow()
            .missed_storage_invocations();
    }
}

impl<S: WriteStorage> VmTracer<S> for StorageInvocations {}
