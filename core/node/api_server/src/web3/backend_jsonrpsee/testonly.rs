//! Test-only extensions for the `jsonrpsee` middleware.

use std::{mem, sync::Mutex};

use zksync_web3_decl::jsonrpsee::{helpers::MethodResponseResult, MethodResponse};

use super::metadata::MethodMetadata;

#[derive(Debug, Clone)]
pub(crate) struct RecordedCall {
    pub metadata: MethodMetadata,
    pub response: MethodResponseResult,
}

/// Test-only JSON-RPC recorded of all calls passing through `MetadataMiddleware`.
#[derive(Debug, Default)]
pub(crate) struct RecordedMethodCalls(Mutex<Vec<RecordedCall>>);

impl RecordedMethodCalls {
    /// Observes response to a call.
    pub fn observe_response(&self, metadata: &MethodMetadata, response: &MethodResponse) {
        self.0
            .lock()
            .expect("recorded calls are poisoned")
            .push(RecordedCall {
                metadata: metadata.clone(),
                response: response.success_or_error,
            });
    }

    /// Takes all currently recorded calls from this tracer.
    pub fn take(&self) -> Vec<RecordedCall> {
        mem::take(&mut *self.0.lock().expect("recorded calls are poisoned"))
    }
}
