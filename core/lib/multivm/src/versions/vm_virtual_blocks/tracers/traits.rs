use crate::{
    interface::{
        dyn_tracers::vm_1_3_3::DynTracer, storage::WriteStorage, tracer::VmExecutionStopReason,
        VmExecutionResultAndLogs,
    },
    vm_virtual_blocks::{
        bootloader_state::BootloaderState,
        old_vm::{history_recorder::HistoryMode, memory::SimpleMemory},
        types::internals::ZkSyncVmState,
    },
};

pub type TracerPointer<S, H> = Box<dyn VmTracer<S, H>>;
/// Run tracer for collecting data during the vm execution cycles
pub trait ExecutionProcessing<S: WriteStorage, H: HistoryMode>:
    DynTracer<S, SimpleMemory<H>> + ExecutionEndTracer<H>
{
    fn initialize_tracer(&mut self, _state: &mut ZkSyncVmState<S, H>) {}
    fn before_cycle(&mut self, _state: &mut ZkSyncVmState<S, H>) {}
    fn after_cycle(
        &mut self,
        _state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &mut BootloaderState,
    ) {
    }
    fn after_vm_execution(
        &mut self,
        _state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &BootloaderState,
        _stop_reason: VmExecutionStopReason,
    ) {
    }
}

/// Stop the vm execution if the tracer conditions are met
pub trait ExecutionEndTracer<H: HistoryMode> {
    // Returns whether the vm execution should stop.
    fn should_stop_execution(&self) -> bool {
        false
    }
}

/// Save the results of the vm execution.
pub trait VmTracer<S: WriteStorage, H: HistoryMode>:
    DynTracer<S, SimpleMemory<H>> + ExecutionEndTracer<H> + ExecutionProcessing<S, H>
{
    fn save_results(&mut self, _result: &mut VmExecutionResultAndLogs) {}
}

pub trait ToTracerPointer<S, H> {
    #[allow(dead_code)]
    fn into_tracer_pointer(self) -> TracerPointer<S, H>;
}

impl<S: WriteStorage, H: HistoryMode, T: VmTracer<S, H> + 'static> ToTracerPointer<S, H> for T {
    fn into_tracer_pointer(self) -> TracerPointer<S, H> {
        Box::new(self)
    }
}
