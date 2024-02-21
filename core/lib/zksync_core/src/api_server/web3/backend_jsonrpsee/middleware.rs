use std::{
    cell::RefCell,
    future::Future,
    mem,
    num::NonZeroU32,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

use governor::{
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use pin_project_lite::pin_project;
use vise::{
    Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, GaugeGuard, Histogram, Metrics,
};
use zksync_types::api;
use zksync_web3_decl::{
    error::Web3Error,
    jsonrpsee::{
        server::middleware::rpc::{layer::ResponseFuture, RpcServiceT},
        types::{error::ErrorCode, ErrorObject, Request},
        MethodResponse,
    },
};

use crate::api_server::web3::metrics::{MethodLabels, API_METRICS};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "transport", rename_all = "snake_case")]
pub(crate) enum Transport {
    Ws,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "api_jsonrpc_backend_batch")]
struct LimitMiddlewareMetrics {
    /// Number of rate-limited requests.
    rate_limited: Family<Transport, Counter>,
    /// Size of batch requests.
    #[metrics(buckets = Buckets::exponential(1.0..=512.0, 2.0))]
    size: Family<Transport, Histogram<usize>>,
    /// Number of requests rejected by the limiter.
    rejected: Family<Transport, Counter>,
}

#[vise::register]
static METRICS: vise::Global<LimitMiddlewareMetrics> = vise::Global::new();

/// A rate-limiting middleware.
///
/// `jsonrpsee` will allocate the instance of this struct once per session.
pub(crate) struct LimitMiddleware<S> {
    inner: S,
    rate_limiter: Option<RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>>,
    transport: Transport,
    _guard: GaugeGuard,
}

impl<S> LimitMiddleware<S> {
    pub(crate) fn new(inner: S, requests_per_minute_limit: Option<NonZeroU32>) -> Self {
        Self {
            inner,
            rate_limiter: requests_per_minute_limit
                .map(|limit| RateLimiter::direct(Quota::per_minute(limit))),
            transport: Transport::Ws,
            _guard: API_METRICS.ws_open_sessions.inc_guard(1),
        }
    }
}

impl<'a, S> RpcServiceT<'a> for LimitMiddleware<S>
where
    S: Send + Sync + RpcServiceT<'a>,
{
    type Future = ResponseFuture<S::Future>;

    fn call(&self, request: Request<'a>) -> Self::Future {
        if let Some(rate_limiter) = &self.rate_limiter {
            let num_requests = NonZeroU32::MIN; // 1 request, no batches possible

            // Note: if required, we can extract data on rate limiting from the error.
            if rate_limiter.check_n(num_requests).is_err() {
                METRICS.rate_limited[&self.transport].inc();

                let rp = MethodResponse::error(
                    request.id,
                    ErrorObject::borrowed(
                        ErrorCode::ServerError(
                            reqwest::StatusCode::TOO_MANY_REQUESTS.as_u16().into(),
                        )
                        .code(),
                        "Too many requests",
                        None,
                    ),
                );
                return ResponseFuture::ready(rp);
            }
        }
        ResponseFuture::future(self.inner.call(request))
    }
}

#[derive(Debug)]
pub(crate) struct MetadataMiddleware<S> {
    inner: S,
}

impl<S> MetadataMiddleware<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<'a, S> RpcServiceT<'a> for MetadataMiddleware<S>
where
    S: Send + Sync + RpcServiceT<'a>,
{
    type Future = Observed<S::Future>;

    fn call(&self, request: Request<'a>) -> Self::Future {
        Observed {
            metadata: MethodMetadata::new(request.method.as_ref().to_owned()),
            inner: self.inner.call(request),
        }
    }
}

pin_project! {
    #[derive(Debug)]
    pub(crate) struct Observed<F> {
        metadata: MethodMetadata,
        #[pin]
        inner: F,
    }
}

impl<F: Future<Output = MethodResponse>> Future for Observed<F> {
    type Output = MethodResponse;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Implemented as a drop guard in order to correctly revert the value even if the wrapped future panics.
        struct Guard<'a> {
            prev: Option<MethodMetadata>,
            current: &'a mut MethodMetadata,
        }

        impl<'a> Guard<'a> {
            fn new(metadata: &'a mut MethodMetadata) -> Self {
                let prev = CURRENT_METHOD
                    .with(|cell| mem::replace(&mut *cell.borrow_mut(), Some(metadata.clone())));
                Self {
                    prev,
                    current: metadata,
                }
            }
        }

        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                CURRENT_METHOD.with(|cell| {
                    *self.current =
                        mem::replace(&mut *cell.borrow_mut(), self.prev.take()).unwrap();
                });
            }
        }

        let projection = self.project();
        let guard = Guard::new(projection.metadata);
        match projection.inner.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(response) => {
                drop(guard);
                projection.metadata.observe_response(&response);
                Poll::Ready(response)
            }
        }
    }
}

thread_local! {
    static CURRENT_METHOD: RefCell<Option<MethodMetadata>> = RefCell::new(None);
}

#[derive(Debug, Clone)]
pub(crate) struct MethodMetadata {
    name: String,
    started_at: Instant,
    pub block_id: Option<api::BlockId>,
    pub block_diff: Option<u32>,
    has_app_error: bool,
}

impl MethodMetadata {
    fn new(name: String) -> Self {
        Self {
            name,
            started_at: Instant::now(),
            block_id: None,
            block_diff: None,
            has_app_error: false,
        }
    }

    /// Accesses metadata for the current method.
    pub fn with(access_fn: impl FnOnce(&mut Self)) {
        CURRENT_METHOD.with(|cell| {
            if let Some(metadata) = &mut *cell.borrow_mut() {
                access_fn(metadata);
            }
        });
    }

    /// Returns the full name of the method, such as `eth_blockNumber`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Sets an application-level error for this method.
    pub fn set_error(&mut self, err: &Web3Error) {
        API_METRICS.observe_web3_error(self.name.clone(), err);
        self.has_app_error = true;
    }

    /// Sets an internal error for this method.
    pub fn set_internal_error(&mut self, error: &str) {
        tracing::error!("Internal error in method {}: {error}", self.name);
        // The error should be set on a subsequent call `Web3Error` conversion, so this is more of a defence.
        self.has_app_error = true;
    }

    fn observe_response(&self, response: &MethodResponse) {
        let mut protocol_error = false;
        if !self.has_app_error {
            // Do not report error code for app-level errors.
            if let Some(error_code) = response.success_or_error.as_error_code() {
                API_METRICS.observe_rpc_error(self.name.clone(), error_code);
                protocol_error = true;
            }
        }

        // Do not report latency for calls with protocol-level errors since it can blow up cardinality
        // of the method name label (it isn't guaranteed to exist).
        if !protocol_error {
            API_METRICS.observe_latency(&MethodLabels::from(self), self.started_at.elapsed());
        }
    }
}

impl From<&MethodMetadata> for MethodLabels {
    fn from(meta: &MethodMetadata) -> Self {
        let mut labels = Self::new(meta.name.clone());
        if let Some(block_id) = meta.block_id {
            labels = labels.with_block_id(block_id);
        }
        if let Some(block_diff) = meta.block_diff {
            labels = labels.with_block_diff(block_diff);
        }
        labels
    }
}
