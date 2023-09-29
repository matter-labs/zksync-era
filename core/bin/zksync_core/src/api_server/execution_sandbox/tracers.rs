use once_cell::sync::OnceCell;
use std::sync::Arc;
use vm::{CallTracer, HistoryMode, VmTracer};
use zksync_state::WriteStorage;
use zksync_types::vm_trace::Call;

/// Custom tracers supported by our api
#[derive(Debug)]
pub(crate) enum ApiTracer {
    CallTracer(Arc<OnceCell<Vec<Call>>>),
}

impl ApiTracer {
    pub fn into_boxed<S: WriteStorage, H: HistoryMode + 'static>(
        self,
        history: H,
    ) -> Box<dyn VmTracer<S, H>> {
        match self {
            ApiTracer::CallTracer(tracer) => Box::new(CallTracer::new(tracer, history)),
        }
    }
}
