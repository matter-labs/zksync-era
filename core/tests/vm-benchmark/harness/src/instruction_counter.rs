use std::{cell::RefCell, rc::Rc};

use zksync_multivm::{
    interface::{dyn_tracers::vm_1_5_0::DynTracer, tracer::TracerExecutionStatus},
    vm_latest::{BootloaderState, HistoryMode, SimpleMemory, VmTracer, ZkSyncVmState},
};
use zksync_state::WriteStorage;

pub struct InstructionCounter {
    count: usize,
    output: Rc<RefCell<usize>>,
}

/// A tracer that counts the number of instructions executed by the VM.
impl InstructionCounter {
    #[allow(dead_code)] // FIXME: re-enable instruction counting once new tracers are merged
    pub fn new(output: Rc<RefCell<usize>>) -> Self {
        Self { count: 0, output }
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for InstructionCounter {
    fn finish_cycle(
        &mut self,
        _state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        self.count += 1;
        TracerExecutionStatus::Continue
    }

    fn after_vm_execution(
        &mut self,
        _state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &BootloaderState,
        _stop_reason: zksync_multivm::interface::tracer::VmExecutionStopReason,
    ) {
        *self.output.borrow_mut() = self.count;
    }
}

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for InstructionCounter {}
