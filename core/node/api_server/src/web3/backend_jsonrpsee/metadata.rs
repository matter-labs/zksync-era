//! Method metadata.

use std::{cell::RefCell, mem, sync::Arc, time::Instant};

use thread_local::ThreadLocal;
use zksync_types::{api, api::state_override::StateOverride};
use zksync_web3_decl::{error::Web3Error, jsonrpsee::MethodResponse};

#[cfg(test)]
use super::testonly::RecordedMethodCalls;
use crate::{
    execution_sandbox::SANDBOX_METRICS,
    tx_sender::SubmitTxError,
    web3::metrics::{ObservedRpcParams, API_METRICS},
};

/// Metadata assigned to a JSON-RPC method call.
#[derive(Debug, Clone)]
pub(crate) struct MethodMetadata {
    pub name: &'static str,
    pub started_at: Instant,
    /// Block ID requested by the call.
    pub block_id: Option<api::BlockId>,
    /// Difference between the latest block number and the requested block ID.
    pub block_diff: Option<u32>,
    /// Did this call return an app-level error?
    pub has_app_error: bool,
}

impl MethodMetadata {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            started_at: Instant::now(),
            block_id: None,
            block_diff: None,
            has_app_error: false,
        }
    }
}

type CurrentMethodInner = RefCell<Option<MethodMetadata>>;

#[must_use = "guard will reset method metadata on drop"]
#[derive(Debug)]
pub(super) struct CurrentMethodGuard<'a> {
    prev: Option<MethodMetadata>,
    current: &'a mut MethodMetadata,
    thread_local: &'a ThreadLocal<CurrentMethodInner>,
}

impl Drop for CurrentMethodGuard<'_> {
    fn drop(&mut self) {
        let cell = self.thread_local.get_or_default();
        *self.current = mem::replace(&mut *cell.borrow_mut(), self.prev.take()).unwrap();
    }
}

/// Tracer of JSON-RPC methods. Can be used to access metadata for the currently handled method call.
// We organize the tracer as a thread-local variable with current method metadata, which is set while the method handler
// is being polled. We use the drop guard pattern to handle corner cases like the handler panicking.
// Method handlers are wrapped using RPC-level middleware in `jsonrpsee`.
#[derive(Debug, Default)]
pub struct MethodTracer {
    inner: ThreadLocal<CurrentMethodInner>,
    #[cfg(test)]
    recorder: RecordedMethodCalls,
}

impl MethodTracer {
    /// Sets the block ID for the current JSON-RPC method call. It will be used as a metric label for method latency etc.
    ///
    /// This should be called inside JSON-RPC method handlers; otherwise, this method is a no-op.
    pub fn set_block_id(&self, block_id: api::BlockId) {
        let cell = self.inner.get_or_default();
        if let Some(metadata) = &mut *cell.borrow_mut() {
            metadata.block_id = Some(block_id);
        }
    }

    /// Sets the difference between the latest sealed L2 block and the requested L2 block for the current JSON-RPC method call.
    /// It will be used as a metric label for method latency etc.
    ///
    /// This should be called inside JSON-RPC method handlers; otherwise, this method is a no-op.
    pub fn set_block_diff(&self, block_diff: u32) {
        let cell = self.inner.get_or_default();
        if let Some(metadata) = &mut *cell.borrow_mut() {
            metadata.block_diff = Some(block_diff);
        }
    }

    /// Observes state override metrics.
    pub fn observe_state_override(&self, state_override: Option<&StateOverride>) {
        let cell = self.inner.get_or_default();
        if let (Some(metadata), Some(state_override)) = (&*cell.borrow(), state_override) {
            SANDBOX_METRICS.observe_override_metrics(metadata.name, state_override);
        }
    }

    pub(super) fn new_call<'a>(
        self: &Arc<Self>,
        name: &'static str,
        raw_params: ObservedRpcParams<'a>,
    ) -> MethodCall<'a> {
        MethodCall {
            tracer: self.clone(),
            params: raw_params,
            meta: MethodMetadata::new(name),
            is_completed: false,
        }
    }

    pub(super) fn observe_error(&self, err: &Web3Error) {
        let cell = self.inner.get_or_default();
        if let Some(metadata) = &mut *cell.borrow_mut() {
            API_METRICS.observe_web3_error(metadata.name, err);
            metadata.has_app_error = true;
        }
    }

    pub(super) fn observe_submit_error(&self, err: &SubmitTxError) {
        let cell = self.inner.get_or_default();
        if let Some(metadata) = &*cell.borrow() {
            API_METRICS.observe_submit_error(metadata.name, err);
        }
    }
}

#[cfg(test)]
impl MethodTracer {
    /// Copies current method metadata.
    pub(crate) fn meta(&self) -> Option<MethodMetadata> {
        self.inner.get_or_default().borrow().clone()
    }

    pub(crate) fn recorded_calls(&self) -> &RecordedMethodCalls {
        &self.recorder
    }
}

#[derive(Debug)]
pub(super) struct MethodCall<'a> {
    tracer: Arc<MethodTracer>,
    meta: MethodMetadata,
    params: ObservedRpcParams<'a>,
    is_completed: bool,
}

impl Drop for MethodCall<'_> {
    fn drop(&mut self) {
        if !self.is_completed {
            API_METRICS.observe_dropped_call(&self.meta, &self.params);
        }
    }
}

impl MethodCall<'_> {
    pub(super) fn set_as_current(&mut self) -> CurrentMethodGuard<'_> {
        let meta = &mut self.meta;
        let cell = self.tracer.inner.get_or_default();
        let prev = (*cell.borrow_mut()).replace(meta.clone());
        CurrentMethodGuard {
            prev,
            current: meta,
            thread_local: &self.tracer.inner,
        }
    }

    pub(super) fn observe_response(&mut self, response: &MethodResponse) {
        self.is_completed = true;
        let meta = &self.meta;
        let params = &self.params;
        match response.as_error_code() {
            None => {
                API_METRICS.observe_response_size(meta.name, params, response.as_result().len());
            }
            Some(error_code) => {
                API_METRICS.observe_protocol_error(
                    meta.name,
                    params,
                    error_code,
                    meta.has_app_error,
                );
            }
        }
        API_METRICS.observe_latency(meta, params);
        #[cfg(test)]
        self.tracer.recorder.observe_response(meta, response);
    }
}
