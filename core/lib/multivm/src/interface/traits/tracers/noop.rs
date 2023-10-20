use crate::interface::dyn_tracers::vm_1_3_3::DynTracer;
use zksync_state::WriteStorage;

pub struct NoopTracer;

impl<S: WriteStorage, H: crate::vm_latest::HistoryMode>
    DynTracer<S, crate::vm_latest::SimpleMemory<H>> for NoopTracer
{
}

impl<S: WriteStorage, H: crate::vm_latest::HistoryMode> crate::vm_latest::VmTracer<S, H>
    for NoopTracer
{
}

impl<S: WriteStorage, H: crate::vm_virtual_blocks::HistoryMode>
    DynTracer<S, crate::vm_virtual_blocks::SimpleMemory<H>> for NoopTracer
{
}

impl<H: crate::vm_virtual_blocks::HistoryMode>
    crate::versions::vm_virtual_blocks::ExecutionEndTracer<H> for NoopTracer
{
}

impl<S: WriteStorage, H: crate::vm_virtual_blocks::HistoryMode>
    crate::versions::vm_virtual_blocks::ExecutionProcessing<S, H> for NoopTracer
{
}

impl<S: WriteStorage, H: crate::vm_virtual_blocks::HistoryMode>
    crate::vm_virtual_blocks::VmTracer<S, H> for NoopTracer
{
}
