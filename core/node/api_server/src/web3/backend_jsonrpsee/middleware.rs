use std::{
    cell::RefCell,
    collections::HashSet,
    future::Future,
    mem,
    num::NonZeroU32,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use governor::{
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use once_cell::sync::OnceCell;
use pin_project_lite::pin_project;
use rand::{rngs::SmallRng, RngCore, SeedableRng};
use tokio::sync::watch;
use tracing::instrument::{Instrument, Instrumented};
use zksync_instrument::alloc::AllocationAccumulator;
use zksync_web3_decl::jsonrpsee::{
    server::middleware::rpc::{layer::ResponseFuture, RpcServiceT},
    types::{error::ErrorCode, ErrorObject, Id, Request},
    MethodResponse,
};

use super::metadata::{MethodCall, MethodTracer};
use crate::web3::metrics::{ApiTransportLabel, ObservedRpcParams, LIMIT_METRICS};

/// A rate-limiting middleware.
///
/// `jsonrpsee` will allocate the instance of this struct once per session.
pub(crate) struct LimitMiddleware<S> {
    inner: S,
    ws_rate_limiter: Option<RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>>,
}

impl<S> LimitMiddleware<S> {
    pub(crate) fn new(inner: S, requests_per_minute_limit: Option<NonZeroU32>) -> Self {
        Self {
            inner,
            ws_rate_limiter: requests_per_minute_limit
                .map(|limit| RateLimiter::direct(Quota::per_minute(limit))),
        }
    }
}

impl<'a, S> RpcServiceT<'a> for LimitMiddleware<S>
where
    S: Send + Sync + RpcServiceT<'a>,
{
    type Future = ResponseFuture<S::Future>;

    fn call(&self, request: Request<'a>) -> Self::Future {
        let transport = *request
            .extensions()
            .get::<ApiTransportLabel>()
            .expect("no transport label");
        if let (ApiTransportLabel::Ws, Some(rate_limiter)) = (transport, &self.ws_rate_limiter) {
            let num_requests = NonZeroU32::MIN; // 1 request (applied on the RPC request level)

            // Note: if required, we can extract data on rate limiting from the error.
            if rate_limiter.check_n(num_requests).is_err() {
                LIMIT_METRICS.rate_limited[&ApiTransportLabel::Ws].inc();

                let rp = MethodResponse::error(
                    request.id,
                    ErrorObject::borrowed(
                        ErrorCode::ServerError(http::StatusCode::TOO_MANY_REQUESTS.as_u16().into())
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

/// RPC-level middleware that adds [`MethodCall`] metadata to method logic. Method handlers can then access this metadata
/// using [`MethodTracer`], which is a part of `RpcState`. When the handler completes or is dropped, the results are reported
/// as metrics.
///
/// As an example, a method handler can set the requested block ID, which would then be used in relevant metric labels.
///
/// # Implementation notes
///
/// We express `TRACE_PARAMS` as a const param rather than a field so that the Rust compiler has more room for optimizations in case tracing
/// is switched off.
#[derive(Debug)]
pub(crate) struct MetadataMiddleware<S, const TRACE_PARAMS: bool> {
    inner: S,
    registered_method_names: Arc<HashSet<&'static str>>,
    method_tracer: Arc<MethodTracer>,
}

impl<'a, S, const TRACE_PARAMS: bool> RpcServiceT<'a> for MetadataMiddleware<S, TRACE_PARAMS>
where
    S: Send + Sync + RpcServiceT<'a>,
{
    type Future = WithMethodCall<'a, S::Future>;

    fn call(&self, request: Request<'a>) -> Self::Future {
        let transport = *request
            .extensions()
            .get::<ApiTransportLabel>()
            .expect("transport label not set");
        // "Normalize" the method name by searching it in the set of all registered methods. This extends the lifetime
        // of the name to `'static` and maps unknown methods to "", so that method name metric labels don't have unlimited cardinality.
        let method_name = self
            .registered_method_names
            .get(request.method_name())
            .copied()
            .unwrap_or("");

        let observed_params = if TRACE_PARAMS {
            ObservedRpcParams::new(request.params.as_ref())
        } else {
            ObservedRpcParams::Unknown
        };
        let call = self
            .method_tracer
            .new_call(transport, method_name, observed_params);
        let mut future = WithMethodCall::new(self.inner.call(request), call);
        if TRACE_PARAMS {
            future.alloc = Some(AllocationAccumulator::new(method_name));
        }
        future
    }
}

pin_project! {
    #[derive(Debug)]
    pub(crate) struct WithMethodCall<'a, F> {
        #[pin]
        inner: F,
        call: MethodCall<'a>,
        alloc: Option<AllocationAccumulator<'static>>,
    }
}

impl<'a, F> WithMethodCall<'a, F> {
    fn new(inner: F, call: MethodCall<'a>) -> Self {
        Self {
            inner,
            call,
            alloc: None,
        }
    }
}

impl<F: Future<Output = MethodResponse>> Future for WithMethodCall<'_, F> {
    type Output = MethodResponse;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let projection = self.project();
        let guard = projection.call.set_as_current();
        let _alloc_guard = projection.alloc.as_mut().map(AllocationAccumulator::start);
        match projection.inner.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(response) => {
                drop(guard);
                projection.call.observe_response(&response);
                Poll::Ready(response)
            }
        }
    }
}

/// [`tower`] middleware layer that wraps services into [`MetadataMiddleware`]. Implemented as a named type
/// to simplify call sites.
///
/// # Implementation notes
///
/// We express `TRACE_PARAMS` as a const param rather than a field so that the Rust compiler has more room for optimizations in case tracing
/// is switched off.
#[derive(Debug, Clone)]
pub(crate) struct MetadataLayer<const TRACE_PARAMS: bool> {
    registered_method_names: Arc<HashSet<&'static str>>,
    method_tracer: Arc<MethodTracer>,
}

impl MetadataLayer<false> {
    pub fn new(
        registered_method_names: Arc<HashSet<&'static str>>,
        method_tracer: Arc<MethodTracer>,
    ) -> Self {
        Self {
            registered_method_names,
            method_tracer,
        }
    }

    pub fn with_param_tracing(self) -> MetadataLayer<true> {
        MetadataLayer {
            registered_method_names: self.registered_method_names,
            method_tracer: self.method_tracer,
        }
    }
}

impl<Svc, const TRACE_PARAMS: bool> tower::Layer<Svc> for MetadataLayer<TRACE_PARAMS> {
    type Service = MetadataMiddleware<Svc, TRACE_PARAMS>;

    fn layer(&self, inner: Svc) -> Self::Service {
        MetadataMiddleware {
            inner,
            registered_method_names: self.registered_method_names.clone(),
            method_tracer: self.method_tracer.clone(),
        }
    }
}

/// Middleware that adds tracing spans to each RPC call, so that logs belonging to the same call
/// can be easily filtered.
#[derive(Debug)]
pub(crate) struct CorrelationMiddleware<S> {
    inner: S,
}

impl<S> CorrelationMiddleware<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<'a, S> RpcServiceT<'a> for CorrelationMiddleware<S>
where
    S: RpcServiceT<'a>,
{
    type Future = Instrumented<S::Future>;

    fn call(&self, request: Request<'a>) -> Self::Future {
        thread_local! {
            static CORRELATION_ID_RNG: RefCell<SmallRng> = RefCell::new(SmallRng::from_entropy());
        }

        // Unlike `MetadataMiddleware`, we don't need to extend the method lifetime to `'static`;
        // `tracing` span instantiation allocates a `String` for supplied `&str`s in any case.
        let method = request.method_name();
        // Wrap a call into a span with unique correlation ID, so that events occurring in the span can be easily filtered.
        // This works as a cheap alternative to Open Telemetry tracing with its trace / span IDs.
        let correlation_id = CORRELATION_ID_RNG.with(|rng| rng.borrow_mut().next_u64());
        let call_span = tracing::debug_span!("rpc_call", method, correlation_id);
        self.inner.call(request).instrument(call_span)
    }
}

/// Tracks the timestamp of the last call to the RPC. Used during server shutdown to start dropping new traffic
/// only after this is coordinated by the external load balancer.
#[derive(Debug, Clone, Default)]
pub(crate) struct TrafficTracker {
    // We use `OnceCell` to not track requests before the server starts shutting down.
    last_call_sender: Arc<OnceCell<watch::Sender<Instant>>>,
}

impl TrafficTracker {
    fn reset(&self) {
        if let Some(last_call) = self.last_call_sender.get() {
            last_call.send_replace(Instant::now());
        }
    }

    /// Waits until no new requests are received during the specified interval.
    pub async fn wait_for_no_requests(self, interval_without_requests: Duration) {
        let mut last_call_subscriber = self
            .last_call_sender
            .get_or_init(|| watch::channel(Instant::now()).0)
            .subscribe();
        // Drop `last_call_sender` to handle the case when the server was dropped for other reasons.
        drop(self);

        let deadline = *last_call_subscriber.borrow() + interval_without_requests;
        let sleep = tokio::time::sleep_until(deadline.into());
        tokio::pin!(sleep);

        loop {
            tokio::select! {
                () = sleep.as_mut() => {
                    return; // Successfully waited for no requests
                }
                change_result = last_call_subscriber.changed() => {
                    if change_result.is_err() {
                        return; // All `ShutdownTimeout` instances are dropped; no point in waiting any longer
                    }
                    let new_deadline = *last_call_subscriber.borrow() + interval_without_requests;
                    sleep.as_mut().reset(new_deadline.into());
                }
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct ShutdownMiddleware<S> {
    inner: S,
    traffic_tracker: TrafficTracker,
}

impl<S> ShutdownMiddleware<S> {
    pub fn new(inner: S, traffic_tracker: TrafficTracker) -> Self {
        Self {
            inner,
            traffic_tracker,
        }
    }
}

impl<'a, S> RpcServiceT<'a> for ShutdownMiddleware<S>
where
    S: Send + Sync + RpcServiceT<'a>,
{
    type Future = S::Future;

    fn call(&self, request: Request<'a>) -> Self::Future {
        self.traffic_tracker.reset();
        self.inner.call(request)
    }
}

/// Middleware aborting any request after a configurable timeout.
#[derive(Debug)]
pub(crate) struct ServerTimeoutMiddleware<S> {
    inner: S,
    timeout: Duration,
}

impl<S> ServerTimeoutMiddleware<S> {
    pub fn new(inner: S, timeout: Duration) -> Self {
        Self { inner, timeout }
    }
}

impl<'a, S> RpcServiceT<'a> for ServerTimeoutMiddleware<S>
where
    S: Send + Sync + RpcServiceT<'a>,
{
    type Future = WithServerTimeout<'a, S::Future>;

    fn call(&self, request: Request<'a>) -> Self::Future {
        let request_id = request.id.clone();
        let inner = tokio::time::timeout(self.timeout, self.inner.call(request));
        WithServerTimeout { request_id, inner }
    }
}

pin_project! {
    #[derive(Debug)]
    pub(crate) struct WithServerTimeout<'a, F> {
        request_id: Id<'a>,
        #[pin]
        inner: tokio::time::Timeout<F>,
    }
}

impl<F: Future<Output = MethodResponse>> Future for WithServerTimeout<'_, F> {
    type Output = MethodResponse;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let projection = self.project();
        match projection.inner.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(response)) => Poll::Ready(response),
            Poll::Ready(Err(_)) => {
                let err_code = http::StatusCode::SERVICE_UNAVAILABLE.as_u16().into();
                let err = ErrorObject::borrowed(
                    err_code,
                    "Request timed out due to server being overloaded",
                    None,
                );
                // `replace()` is safe: the future is not polled after it returns `Poll::Ready`
                let id = mem::replace(projection.request_id, Id::Null);
                Poll::Ready(MethodResponse::error(id, err))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RpcMethodFilterConfig {
    pub http_only_methods: HashSet<&'static str>,
    pub ws_only_methods: HashSet<&'static str>,
}

/// Filtering for RPC methods based on the method name. Required because otherwise subscriptions will return
/// internal errors when called via HTTP.
#[derive(Debug)]
pub(crate) struct RpcMethodFilter<S> {
    inner: S,
    config: Arc<RpcMethodFilterConfig>,
}

impl<Svc> RpcMethodFilter<Svc> {
    pub fn new(inner: Svc, config: Arc<RpcMethodFilterConfig>) -> Self {
        Self { inner, config }
    }
}

impl<'a, S> RpcServiceT<'a> for RpcMethodFilter<S>
where
    S: Send + Sync + RpcServiceT<'a>,
{
    type Future = ResponseFuture<S::Future>;

    fn call(&self, request: Request<'a>) -> Self::Future {
        let transport = *request
            .extensions()
            .get::<ApiTransportLabel>()
            .expect("no transport label");
        let method = request.method.as_ref();
        let should_reject = match transport {
            ApiTransportLabel::Http => self.config.ws_only_methods.contains(&method),
            ApiTransportLabel::Ws => self.config.http_only_methods.contains(&method),
        };
        if should_reject {
            let err = MethodResponse::error(
                request.id,
                ErrorObject::borrowed(
                    ErrorCode::MethodNotFound.code(),
                    ErrorCode::MethodNotFound.message(),
                    None,
                ),
            );
            ResponseFuture::ready(err)
        } else {
            ResponseFuture::future(self.inner.call(request))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rand::{thread_rng, Rng};
    use test_casing::{test_casing, Product};
    use zksync_types::api;
    use zksync_web3_decl::jsonrpsee::{types::Id, ResponsePayload};

    use super::*;

    #[test_casing(4, Product(([false, true], [false, true])))]
    #[tokio::test(flavor = "multi_thread")]
    async fn metadata_middleware_basics(spawn_tasks: bool, sleep: bool) {
        let method_tracer = Arc::new(MethodTracer::default());

        let tasks = (0_u64..100).map(|i| {
            let current_method = method_tracer.clone();
            let inner = async move {
                assert_eq!(current_method.meta().unwrap().block_id, None);
                current_method.set_block_id(api::BlockId::Number(i.into()));

                for diff in 0_u32..10 {
                    let meta = current_method.meta().unwrap();
                    assert_eq!(meta.block_id, Some(api::BlockId::Number(i.into())));
                    assert_eq!(meta.block_diff, diff.checked_sub(1));
                    current_method.set_block_diff(diff);

                    if sleep {
                        let delay = thread_rng().gen_range(1..=5);
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    } else {
                        tokio::task::yield_now().await;
                    }
                }

                MethodResponse::response(
                    Id::Number(1),
                    ResponsePayload::success("{}".to_string()),
                    usize::MAX,
                )
            };

            WithMethodCall::new(
                inner,
                method_tracer.new_call(ApiTransportLabel::Http, "test", ObservedRpcParams::None),
            )
        });

        if spawn_tasks {
            let tasks: Vec<_> = tasks.map(tokio::spawn).collect();
            for task in tasks {
                task.await.unwrap();
            }
        } else {
            futures::future::join_all(tasks).await;
        }

        let calls = method_tracer.recorded_calls().take();
        assert_eq!(calls.len(), 100);
        for call in &calls {
            assert_eq!(call.metadata.name, "test");
            assert!(call.metadata.block_id.is_some());
            assert_eq!(call.metadata.block_diff, Some(9));
            assert!(call.error_code.is_none());
        }
    }

    #[tokio::test]
    async fn traffic_tracker_basics() {
        let traffic_tracker = TrafficTracker::default();
        let now = Instant::now();
        let wait = traffic_tracker
            .clone()
            .wait_for_no_requests(Duration::from_millis(10));
        tokio::time::sleep(Duration::from_millis(5)).await;
        traffic_tracker.reset();
        wait.await;

        let elapsed = now.elapsed();
        assert!(elapsed >= Duration::from_millis(15), "{elapsed:?}");
    }
}
