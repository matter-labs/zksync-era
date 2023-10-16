use crate::{BootloaderState, HistoryMode, SimpleMemory, VmExecutionStopReason, ZkSyncVmState};
use std::marker::PhantomData;
use vm_tracer_interface::traits::vm_1_3_3::{DynTracer, VmTracer};
use zksync_state::WriteStorage;

#[derive(Default)]
pub struct NoopTracer<H> {
    _h: PhantomData<H>,
}

impl<S: WriteStorage, H: HistoryMode> DynTracer<S> for NoopTracer<H> {
    type Memory = SimpleMemory<H>;
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S> for NoopTracer<H> {
    type ZkSyncVmState = ZkSyncVmState<S, H>;
    type BootloaderState = BootloaderState;
    type VmExecutionStopReason = VmExecutionStopReason;
}
