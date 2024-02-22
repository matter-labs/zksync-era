use std::{
    cell::RefCell,
    collections::HashSet,
    future::Future,
    mem,
    num::NonZeroU32,
    pin::Pin,
    sync::Arc,
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

#[cfg(test)]
use super::testonly::CallTracer;
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
    registered_method_names: Arc<HashSet<&'static str>>,
    #[cfg(test)]
    call_tracer: CallTracer,
}

impl<S> MetadataMiddleware<S> {
    pub fn new(inner: S, registered_method_names: Arc<HashSet<&'static str>>) -> Self {
        Self {
            inner,
            registered_method_names,
            #[cfg(test)]
            call_tracer: CallTracer::default(),
        }
    }

    #[cfg(test)]
    pub fn with_call_tracer(mut self, call_tracer: Option<CallTracer>) -> Self {
        if let Some(call_tracer) = call_tracer {
            self.call_tracer = call_tracer;
        }
        self
    }
}

impl<'a, S> RpcServiceT<'a> for MetadataMiddleware<S>
where
    S: Send + Sync + RpcServiceT<'a>,
{
    type Future = WithMethodMetadata<S::Future>;

    fn call(&self, request: Request<'a>) -> Self::Future {
        let method_name = self
            .registered_method_names
            .get(request.method_name())
            .copied()
            .unwrap_or("");

        WithMethodMetadata {
            metadata: MethodMetadataGuard::new(
                method_name,
                #[cfg(test)]
                self.call_tracer.clone(),
            ),
            inner: self.inner.call(request),
        }
    }
}

pin_project! {
    #[derive(Debug)]
    pub(crate) struct WithMethodMetadata<F> {
        metadata: MethodMetadataGuard,
        #[pin]
        inner: F,
    }
}

impl<F: Future<Output = MethodResponse>> Future for WithMethodMetadata<F> {
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
        let guard = Guard::new(&mut projection.metadata.inner);
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

/// Metadata assigned to a JSON-RPC method call.
#[derive(Debug, Clone)]
pub(crate) struct MethodMetadata {
    name: &'static str,
    started_at: Instant,
    /// Block ID requested by the call.
    pub block_id: Option<api::BlockId>,
    /// Difference between the latest block number and the requested block ID.
    pub block_diff: Option<u32>,
    /// Did this call return an app-level error?
    has_app_error: bool,
}

impl MethodMetadata {
    /// Accesses metadata for the current method. Should not be called recursively.
    pub fn with(access_fn: impl FnOnce(&mut Self)) {
        CURRENT_METHOD.with(|cell| {
            if let Some(metadata) = &mut *cell.borrow_mut() {
                access_fn(metadata);
            }
        });
    }

    #[cfg(test)]
    pub fn name(&self) -> &str {
        self.name
    }

    #[cfg(test)]
    pub fn has_app_error(&self) -> bool {
        self.has_app_error
    }

    /// Sets an application-level error for this method.
    pub fn observe_error(&mut self, err: &Web3Error) {
        API_METRICS.observe_web3_error(self.name, err);
        self.has_app_error = true;
    }

    fn observe_response(&self, response: &MethodResponse) {
        if let Some(error_code) = response.success_or_error.as_error_code() {
            API_METRICS.observe_protocol_error(self.name, error_code, self.has_app_error);
        }

        // Do not report latency for calls with protocol-level errors since it can blow up cardinality
        // of the method name label (it isn't guaranteed to exist).
        if response.success_or_error.is_success() || self.has_app_error {
            API_METRICS.observe_latency(&MethodLabels::from(self), self.started_at.elapsed());
        }
    }
}

impl From<&MethodMetadata> for MethodLabels {
    fn from(meta: &MethodMetadata) -> Self {
        let mut labels = Self::new(meta.name);
        if let Some(block_id) = meta.block_id {
            labels = labels.with_block_id(block_id);
        }
        if let Some(block_diff) = meta.block_diff {
            labels = labels.with_block_diff(block_diff);
        }
        labels
    }
}

#[derive(Debug)]
struct MethodMetadataGuard {
    inner: MethodMetadata,
    is_completed: bool,
    #[cfg(test)]
    call_tracer: CallTracer,
}

impl Drop for MethodMetadataGuard {
    fn drop(&mut self) {
        if !self.is_completed {
            API_METRICS.observe_dropped_call(
                &MethodLabels::from(&self.inner),
                self.inner.started_at.elapsed(),
            );
        }
    }
}

impl MethodMetadataGuard {
    fn new(name: &'static str, #[cfg(test)] call_tracer: CallTracer) -> Self {
        let inner = MethodMetadata {
            name,
            started_at: Instant::now(),
            block_id: None,
            block_diff: None,
            has_app_error: false,
        };
        Self {
            inner,
            is_completed: false,
            #[cfg(test)]
            call_tracer,
        }
    }

    fn observe_response(&mut self, response: &MethodResponse) {
        self.is_completed = true;
        self.inner.observe_response(response);
        #[cfg(test)]
        self.call_tracer.observe_response(&self.inner, response);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use jsonrpsee::helpers::MethodResponseResult;
    use rand::{thread_rng, Rng};
    use test_casing::{test_casing, Product};

    use super::*;

    #[test_casing(4, Product(([false, true], [false, true])))]
    #[tokio::test(flavor = "multi_thread")]
    async fn metadata_middleware_basics(spawn_tasks: bool, sleep: bool) {
        let calls = CallTracer::default();

        let tasks = (0_u64..100).map(|i| {
            let calls = calls.clone();
            let inner = async move {
                MethodMetadata::with(|meta| {
                    assert_eq!(meta.block_id, None);
                    meta.block_id = Some(api::BlockId::Number(i.into()));
                });

                for diff in 0_u32..10 {
                    MethodMetadata::with(|meta| {
                        assert_eq!(meta.block_id, Some(api::BlockId::Number(i.into())));
                        assert_eq!(meta.block_diff, diff.checked_sub(1));
                        meta.block_diff = Some(diff);
                    });

                    if sleep {
                        let delay = thread_rng().gen_range(1..=5);
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    } else {
                        tokio::task::yield_now().await;
                    }
                }

                MethodResponse {
                    result: "{}".to_string(),
                    success_or_error: MethodResponseResult::Success,
                    is_subscription: false,
                }
            };

            WithMethodMetadata {
                metadata: MethodMetadataGuard::new("test", calls),
                inner,
            }
        });

        if spawn_tasks {
            let tasks: Vec<_> = tasks.map(tokio::spawn).collect();
            for task in tasks {
                task.await.unwrap();
            }
        } else {
            futures::future::join_all(tasks).await;
        }

        let calls = calls.take();
        assert_eq!(calls.len(), 100);
        for call in &calls {
            assert_eq!(call.metadata.name, "test");
            assert!(call.metadata.block_id.is_some());
            assert_eq!(call.metadata.block_diff, Some(9));
            assert!(call.response.is_success());
        }
    }
}
