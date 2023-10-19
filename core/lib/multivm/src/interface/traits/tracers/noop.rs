use crate::interface::dyn_tracers::vm_1_3_3::DynTracer;
use crate::vm_latest::{HistoryMode, SimpleMemory, VmTracer};
use zksync_state::WriteStorage;

pub struct NoopTracer;

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for NoopTracer {}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for NoopTracer {}
