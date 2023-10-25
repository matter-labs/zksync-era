use crate::interface::dyn_tracers::vm_1_3_3::DynTracer;
use crate::interface::tracer::VmExecutionStopReason;
use crate::interface::VmExecutionResultAndLogs;
use zksync_state::WriteStorage;

use crate::vm_virtual_blocks::bootloader_state::BootloaderState;
use crate::vm_virtual_blocks::old_vm::history_recorder::HistoryMode;
use crate::vm_virtual_blocks::old_vm::memory::SimpleMemory;
use crate::vm_virtual_blocks::types::internals::ZkSyncVmState;

/// Run tracer for collecting data during the vm execution cycles
#[auto_impl::auto_impl(&mut, Box)]
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
#[auto_impl::auto_impl(&mut, Box)]
pub trait ExecutionEndTracer<H: HistoryMode> {
    // Returns whether the vm execution should stop.
    fn should_stop_execution(&self) -> bool {
        false
    }
}

/// Save the results of the vm execution.
#[auto_impl::auto_impl(&mut, Box)]
pub trait VmTracer<S: WriteStorage, H: HistoryMode>:
    DynTracer<S, SimpleMemory<H>> + ExecutionEndTracer<H> + ExecutionProcessing<S, H>
{
    fn save_results(&mut self, _result: &mut VmExecutionResultAndLogs) {}
}

pub trait BoxedTracer<S, H> {
    fn into_boxed(self) -> Box<dyn VmTracer<S, H>>;
}

impl<S: WriteStorage, H: HistoryMode, T: VmTracer<S, H> + 'static> BoxedTracer<S, H> for T {
    fn into_boxed(self) -> Box<dyn VmTracer<S, H>> {
        Box::new(self)
    }
}
