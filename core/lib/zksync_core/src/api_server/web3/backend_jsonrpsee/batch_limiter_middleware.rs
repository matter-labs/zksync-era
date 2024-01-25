use std::num::NonZeroU32;

use governor::{
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use vise::{
    Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, GaugeGuard, Histogram, Metrics,
};
use zksync_web3_decl::jsonrpsee::{
    server::middleware::rpc::{layer::ResponseFuture, RpcServiceT},
    types::{error::ErrorCode, ErrorObject, Request},
    MethodResponse,
};

use crate::api_server::web3::metrics::API_METRICS;

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
    S: Send + Clone + Sync + RpcServiceT<'a>,
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
