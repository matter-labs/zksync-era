use std::sync::Arc;

use once_cell::sync::OnceCell;
use zksync_multivm::{
    interface::{storage::WriteStorage, Call},
    tracers::{CallTracer, ValidationTracer, ValidationTracerParams, ViolatedValidationRule},
    vm_latest::HistoryDisabled,
    MultiVMTracer, MultiVmTracerPointer,
};
use zksync_types::ProtocolVersionId;

/// Custom tracers supported by the API sandbox.
#[derive(Debug)]
pub(crate) enum ApiTracer {
    CallTracer(Arc<OnceCell<Vec<Call>>>),
    Validation {
        params: ValidationTracerParams,
        result: Arc<OnceCell<ViolatedValidationRule>>,
    },
}

impl ApiTracer {
    pub fn validation(
        params: ValidationTracerParams,
    ) -> (Self, Arc<OnceCell<ViolatedValidationRule>>) {
        let result = Arc::<OnceCell<_>>::default();
        let this = Self::Validation {
            params,
            result: result.clone(),
        };
        (this, result)
    }

    pub(super) fn into_boxed<S>(
        self,
        protocol_version: ProtocolVersionId,
    ) -> MultiVmTracerPointer<S, HistoryDisabled>
    where
        S: WriteStorage,
    {
        match self {
            Self::CallTracer(traces) => CallTracer::new(traces).into_tracer_pointer(),
            Self::Validation { params, result } => {
                let (mut tracer, _) =
                    ValidationTracer::<HistoryDisabled>::new(params, protocol_version.into());
                tracer.result = result;
                tracer.into_tracer_pointer()
            }
        }
    }
}
