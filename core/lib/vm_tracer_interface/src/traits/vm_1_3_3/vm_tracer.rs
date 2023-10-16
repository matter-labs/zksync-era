use crate::traits::vm_1_3_3::DynTracer;
use crate::types::TracerExecutionStatus;
use zksync_state::{StoragePtr, WriteStorage};

/// Run tracer for collecting data during the vm execution cycles
pub trait VmTracer<S: WriteStorage>: DynTracer<S> {
    type ZkSyncVmState;
    type BootloaderState;
    type VmExecutionStopReason;
    /// Initialize the tracer before the vm execution
    fn initialize_tracer(&mut self, _state: &mut Self::ZkSyncVmState) {}
    /// Run after each vm execution cycle
    fn finish_cycle(
        &mut self,
        _state: &mut Self::ZkSyncVmState,
        _bootloader_state: &mut Self::BootloaderState,
    ) -> TracerExecutionStatus {
        TracerExecutionStatus::Continue
    }
    /// Run after the vm execution
    fn after_vm_execution(
        &mut self,
        _state: &mut Self::ZkSyncVmState,
        _bootloader_state: &Self::BootloaderState,
        _stop_reason: Self::VmExecutionStopReason,
    ) {
    }
}

// pub trait BoxedTracer<S, H> {
//     fn into_boxed(self) -> Box<dyn VmTracer<S, H>>;
// }
//
// impl<S: WriteStorage, H: HistoryMode, T: VmTracer<S, H> + 'static> BoxedTracer<S, H> for T {
//     fn into_boxed(self) -> Box<dyn VmTracer<S, H>> {
//         Box::new(self)
//     }
// }
