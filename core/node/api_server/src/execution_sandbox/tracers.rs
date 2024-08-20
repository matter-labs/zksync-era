use std::sync::Arc;

use once_cell::sync::OnceCell;
use zksync_multivm::{
    interface::{storage::WriteStorage, Call},
    tracers::CallTracer,
    vm_latest::HistoryMode,
    MultiVMTracer, MultiVmTracerPointer,
};

/// Custom tracers supported by our API
#[derive(Debug)]
pub enum ApiTracer {
    CallTracer(Arc<OnceCell<Vec<Call>>>),
}

impl ApiTracer {
    pub fn into_boxed<
        S: WriteStorage,
        H: HistoryMode + zksync_multivm::HistoryMode<Vm1_5_0 = H> + 'static,
    >(
        self,
    ) -> MultiVmTracerPointer<S, H> {
        match self {
            ApiTracer::CallTracer(tracer) => CallTracer::new(tracer.clone()).into_tracer_pointer(),
        }
    }
}
