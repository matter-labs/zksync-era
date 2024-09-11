//! Test-only extensions for the `jsonrpsee` middleware.

use std::{mem, sync::Mutex};

use zksync_web3_decl::jsonrpsee::MethodResponse;

use super::metadata::MethodMetadata;

#[derive(Debug, Clone)]
pub(crate) struct RecordedCall {
    pub metadata: MethodMetadata,
    pub error_code: Option<i32>,
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
                error_code: response.as_error_code(),
            });
    }

    /// Takes all currently recorded calls from this tracer.
    pub fn take(&self) -> Vec<RecordedCall> {
        mem::take(&mut *self.0.lock().expect("recorded calls are poisoned"))
    }
}
