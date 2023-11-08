use multivm::tracers::CallTracer;
use multivm::vm_latest::HistoryMode;
use multivm::{MultiVmTracerPointer, MultivmTracer};
use once_cell::sync::OnceCell;

use std::sync::Arc;
use zksync_state::WriteStorage;
use zksync_types::vm_trace::Call;

/// Custom tracers supported by our api
#[derive(Debug)]
pub(crate) enum ApiTracer {
    CallTracer(Arc<OnceCell<Vec<Call>>>),
}

impl ApiTracer {
    pub fn into_boxed<
        S: WriteStorage,
        H: HistoryMode + multivm::HistoryMode<VmBoojumIntegration = H> + 'static,
    >(
        self,
    ) -> MultiVmTracerPointer<S, H> {
        match self {
            ApiTracer::CallTracer(tracer) => CallTracer::new(tracer.clone()).into_tracer_pointer(),
        }
    }
}
