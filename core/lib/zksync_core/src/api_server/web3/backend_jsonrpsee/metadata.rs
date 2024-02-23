//! Method metadata.

use std::{cell::RefCell, mem, sync::Arc, time::Instant};

use thread_local::ThreadLocal;
use zksync_types::api;
use zksync_web3_decl::{error::Web3Error, jsonrpsee::MethodResponse};

#[cfg(test)]
use super::testonly::RecordedMethodCalls;
use crate::api_server::web3::metrics::API_METRICS;

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

#[derive(Debug, Default)]
pub(crate) struct MethodTracer {
    inner: ThreadLocal<CurrentMethodInner>,
    #[cfg(test)]
    recorder: RecordedMethodCalls,
}

impl MethodTracer {
    pub fn set_block_id(&self, block_id: api::BlockId) {
        let cell = self.inner.get_or_default();
        if let Some(metadata) = &mut *cell.borrow_mut() {
            metadata.block_id = Some(block_id);
        }
    }

    pub fn set_block_diff(&self, block_diff: u32) {
        let cell = self.inner.get_or_default();
        if let Some(metadata) = &mut *cell.borrow_mut() {
            metadata.block_diff = Some(block_diff);
        }
    }

    pub(super) fn new_call(self: &Arc<Self>, name: &'static str) -> MethodCall {
        MethodCall {
            current_method: self.clone(),
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
}

#[cfg(test)]
impl MethodTracer {
    /// Copies current method metadata.
    pub fn meta(&self) -> Option<MethodMetadata> {
        self.inner.get_or_default().borrow().clone()
    }

    pub fn recorded_calls(&self) -> &RecordedMethodCalls {
        &self.recorder
    }
}

#[derive(Debug)]
pub(super) struct MethodCall {
    current_method: Arc<MethodTracer>,
    meta: MethodMetadata,
    is_completed: bool,
}

impl Drop for MethodCall {
    fn drop(&mut self) {
        if !self.is_completed {
            API_METRICS.observe_dropped_call(&self.meta);
        }
    }
}

impl MethodCall {
    pub(super) fn set_as_current(&mut self) -> CurrentMethodGuard<'_> {
        let meta = &mut self.meta;
        let cell = self.current_method.inner.get_or_default();
        let prev = mem::replace(&mut *cell.borrow_mut(), Some(meta.clone()));
        CurrentMethodGuard {
            prev,
            current: meta,
            thread_local: &self.current_method.inner,
        }
    }

    pub(super) fn observe_response(&mut self, response: &MethodResponse) {
        self.is_completed = true;
        let meta = &self.meta;
        if let Some(error_code) = response.success_or_error.as_error_code() {
            API_METRICS.observe_protocol_error(meta.name, error_code, meta.has_app_error);
        }
        API_METRICS.observe_latency(meta);
        #[cfg(test)]
        self.current_method
            .recorder
            .observe_response(meta, response);
    }
}
