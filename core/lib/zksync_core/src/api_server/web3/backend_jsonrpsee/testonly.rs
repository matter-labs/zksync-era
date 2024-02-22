//! Test-only extensions for the `jsonrpsee` middleware.

use std::{
    mem,
    sync::{Arc, Mutex},
};

use jsonrpsee::{helpers::MethodResponseResult, MethodResponse};

use super::MethodMetadata;

#[derive(Debug, Clone)]
pub(crate) struct RecordedCall {
    pub metadata: MethodMetadata,
    pub response: MethodResponseResult,
}

/// Test-only JSON-RPC call tracer recording all calls passing through `MetadataMiddleware`.
#[derive(Debug, Clone, Default)]
pub(crate) struct CallTracer(Arc<Mutex<Vec<RecordedCall>>>);

impl CallTracer {
    /// Observes response to a call.
    pub fn observe_response(&self, metadata: &MethodMetadata, response: &MethodResponse) {
        self.0
            .lock()
            .expect("call tracer is poisoned")
            .push(RecordedCall {
                metadata: metadata.clone(),
                response: response.success_or_error,
            });
    }

    /// Takes all currently recorded calls from this tracer.
    pub fn take(&self) -> Vec<RecordedCall> {
        mem::take(&mut *self.0.lock().expect("calls tracer is poisoned"))
    }
}
