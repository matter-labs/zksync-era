use std::sync::Arc;

use multivm::{tracers::CallTracer, vm_latest::HistoryMode, MultiVMTracer, MultiVmTracerPointer};
use once_cell::sync::OnceCell;
use zksync_state::WriteStorage;
use zksync_types::vm_trace::Call;

/// Custom tracers supported by our API
#[derive(Debug)]
pub(crate) enum ApiTracer {
    CallTracer(Arc<OnceCell<Vec<Call>>>),
}

impl ApiTracer {
    pub fn into_boxed<
        S: WriteStorage,
        H: HistoryMode + multivm::HistoryMode<Vm1_4_1 = H> + 'static,
    >(
        self,
    ) -> MultiVmTracerPointer<S, H> {
        match self {
            ApiTracer::CallTracer(tracer) => CallTracer::new(tracer.clone()).into_tracer_pointer(),
        }
    }
}
