use std::{cell::RefCell, rc::Rc};

use zksync_multivm::{
    interface::{storage::WriteStorage, tracer::TracerExecutionStatus},
    tracers::dynamic::vm_1_5_2::DynTracer,
    vm_latest::{BootloaderState, HistoryMode, SimpleMemory, VmTracer, ZkSyncVmState},
};

pub struct InstructionCounter {
    count: usize,
    output: Rc<RefCell<usize>>,
}

/// A tracer that counts the number of instructions executed by the VM.
impl InstructionCounter {
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
