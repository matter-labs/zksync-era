use crate::{HistoryMode, MultiVmTracerPointer};
use zksync_state::WriteStorage;

/// Tracer dispatcher is a tracer that can dispatch calls to multiple tracers.
pub struct TracerDispatcher<S, H> {
    tracers: Vec<MultiVmTracerPointer<S, H>>,
}

impl<S: WriteStorage, H: HistoryMode> From<MultiVmTracerPointer<S, H>> for TracerDispatcher<S, H> {
    fn from(value: MultiVmTracerPointer<S, H>) -> Self {
        Self {
            tracers: vec![value],
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> From<Vec<MultiVmTracerPointer<S, H>>>
    for TracerDispatcher<S, H>
{
    fn from(value: Vec<MultiVmTracerPointer<S, H>>) -> Self {
        Self { tracers: value }
    }
}

impl<S: WriteStorage, H: HistoryMode> Default for TracerDispatcher<S, H> {
    fn default() -> Self {
        Self { tracers: vec![] }
    }
}

impl<S: WriteStorage, H: HistoryMode> From<TracerDispatcher<S, H>>
    for crate::vm_latest::TracerDispatcher<S, H::VmBoojumIntegration>
{
    fn from(value: TracerDispatcher<S, H>) -> Self {
        Self::new(value.tracers.into_iter().map(|x| x.latest()).collect())
    }
}

impl<S: WriteStorage, H: HistoryMode> From<TracerDispatcher<S, H>>
    for crate::vm_refunds_enhancement::TracerDispatcher<S, H::VmVirtualBlocksRefundsEnhancement>
{
    fn from(value: TracerDispatcher<S, H>) -> Self {
        Self::new(
            value
                .tracers
                .into_iter()
                .map(|x| x.vm_refunds_enhancement())
                .collect(),
        )
    }
}

impl<S: WriteStorage, H: HistoryMode> From<TracerDispatcher<S, H>>
    for crate::vm_virtual_blocks::TracerDispatcher<S, H::VmVirtualBlocksMode>
{
    fn from(value: TracerDispatcher<S, H>) -> Self {
        Self::new(
            value
                .tracers
                .into_iter()
                .map(|x| x.vm_virtual_blocks())
                .collect(),
        )
    }
}

/// This is a hack to make `TracerDispatcher` work with VMs, where we don't support tracers.
impl<S, H> From<TracerDispatcher<S, H>> for () {
    fn from(_value: TracerDispatcher<S, H>) -> Self {}
}
