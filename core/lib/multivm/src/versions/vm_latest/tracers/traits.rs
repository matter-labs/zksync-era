use crate::{
    interface::{
        storage::WriteStorage,
        tracer::{TracerExecutionStatus, VmExecutionStopReason},
    },
    tracers::dynamic::vm_1_5_2::DynTracer,
    vm_latest::{
        bootloader::BootloaderState,
        old_vm::{history_recorder::HistoryMode, memory::SimpleMemory},
        types::ZkSyncVmState,
    },
};

pub type TracerPointer<S, H> = Box<dyn VmTracer<S, H>>;

/// Run tracer for collecting data during the vm execution cycles
pub trait VmTracer<S: WriteStorage, H: HistoryMode>: DynTracer<S, SimpleMemory<H>> {
    /// Initialize the tracer before the vm execution
    fn initialize_tracer(&mut self, _state: &mut ZkSyncVmState<S, H>) {}
    /// Run after each vm execution cycle
    fn finish_cycle(
        &mut self,
        _state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        TracerExecutionStatus::Continue
    }
    /// Run after the vm execution
    fn after_vm_execution(
        &mut self,
        _state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &BootloaderState,
        _stop_reason: VmExecutionStopReason,
    ) {
    }
}

pub trait ToTracerPointer<S, H> {
    fn into_tracer_pointer(self) -> TracerPointer<S, H>;
}

impl<S: WriteStorage, H: HistoryMode, T: VmTracer<S, H> + 'static> ToTracerPointer<S, H> for T {
    fn into_tracer_pointer(self) -> TracerPointer<S, H> {
        Box::new(self)
    }
}
