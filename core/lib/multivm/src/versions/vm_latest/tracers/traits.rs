use crate::interface::tracer::VmExecutionStopReason;
use crate::interface::traits::tracers::dyn_tracers::vm_1_3_3::DynTracer;
use crate::interface::types::tracer::TracerExecutionStatus;
use zksync_state::WriteStorage;

use crate::vm_latest::bootloader_state::BootloaderState;
use crate::vm_latest::old_vm::history_recorder::HistoryMode;
use crate::vm_latest::old_vm::memory::SimpleMemory;
use crate::vm_latest::types::internals::ZkSyncVmState;

/// Run tracer for collecting data during the vm execution cycles
#[auto_impl::auto_impl(&mut, Box)]
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

pub trait BoxedTracer<S, H> {
    fn into_boxed(self) -> Box<dyn VmTracer<S, H>>;
}

impl<S: WriteStorage, H: HistoryMode, T: VmTracer<S, H> + 'static> BoxedTracer<S, H> for T {
    fn into_boxed(self) -> Box<dyn VmTracer<S, H>> {
        Box::new(self)
    }
}
