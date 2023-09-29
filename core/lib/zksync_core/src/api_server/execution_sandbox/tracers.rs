use multivm::MultivmTracer;
use once_cell::sync::OnceCell;
use std::sync::Arc;
use vm::CallTracer;
use zksync_state::WriteStorage;
use zksync_types::vm_trace::Call;

/// Custom tracers supported by our api
#[derive(Debug)]
pub(crate) enum ApiTracer {
    CallTracer(Arc<OnceCell<Vec<Call>>>),
}

impl ApiTracer {
    pub fn into_boxed<S: WriteStorage, H: multivm::HistoryMode>(
        self,
    ) -> Box<dyn MultivmTracer<S, H>> {
        match self {
            ApiTracer::CallTracer(tracer) => CallTracer::new(tracer).into_boxed(),
        }
    }
}
