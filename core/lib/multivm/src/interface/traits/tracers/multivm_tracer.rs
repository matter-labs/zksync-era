use crate::HistoryMode;
use zksync_state::WriteStorage;

pub trait MultivmTracer<S: WriteStorage, H: HistoryMode>:
    crate::vm_latest::VmTracer<S, H::VmVirtualBlocksRefundsEnhancement>
    + crate::vm_virtual_blocks::VmTracer<S, H::VmVirtualBlocksMode>
{
    fn into_boxed(self) -> Box<dyn MultivmTracer<S, H>>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

impl<S, H, T> MultivmTracer<S, H> for T
where
    S: WriteStorage,
    H: HistoryMode,
    T: crate::vm_latest::VmTracer<S, H::VmVirtualBlocksRefundsEnhancement>
        + crate::vm_virtual_blocks::VmTracer<S, H::VmVirtualBlocksMode>,
{
}
