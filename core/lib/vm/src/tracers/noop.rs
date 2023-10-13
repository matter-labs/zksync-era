use crate::{DynTracer, HistoryMode, VmTracer};
use zksync_state::WriteStorage;

pub struct NoopTracer;

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, H> for NoopTracer {}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for NoopTracer {}
