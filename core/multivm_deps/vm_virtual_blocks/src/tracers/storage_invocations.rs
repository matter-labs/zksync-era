use crate::bootloader_state::BootloaderState;
use crate::old_vm::history_recorder::HistoryMode;
use crate::tracers::traits::{DynTracer, ExecutionEndTracer, ExecutionProcessing, VmTracer};
use crate::types::internals::ZkSyncVmState;
use zksync_state::WriteStorage;

#[derive(Debug, Default)]
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
impl<S, H: HistoryMode> DynTracer<S, H> for StorageInvocations {}

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
