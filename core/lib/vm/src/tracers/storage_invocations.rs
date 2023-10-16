use crate::{BootloaderState, HistoryMode, SimpleMemory, ZkSyncVmState};
use std::marker::PhantomData;
use vm_tracer_interface::traits::vm_1_3_3::{DynTracer, VmTracer};
use vm_tracer_interface::types::{Halt, TracerExecutionStatus, TracerExecutionStopReason};
use zksync_state::WriteStorage;

#[derive(Debug, Default, Clone)]
pub struct StorageInvocations<H> {
    pub limit: usize,
    _h: PhantomData<H>,
}

impl<H> StorageInvocations<H> {
    pub fn new(limit: usize, _history: H) -> Self {
        Self {
            limit,
            _h: Default::default(),
        }
    }
}

/// Tracer responsible for calculating the number of storage invocations and
/// stopping the VM execution if the limit is reached.
impl<S: WriteStorage, H: HistoryMode> DynTracer<S> for StorageInvocations<H> {
    type Memory = SimpleMemory<H>;
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S> for StorageInvocations<H> {
    type ZkSyncVmState = ZkSyncVmState<S, H>;
    type BootloaderState = BootloaderState;
    type VmExecutionStopReason = ();

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
