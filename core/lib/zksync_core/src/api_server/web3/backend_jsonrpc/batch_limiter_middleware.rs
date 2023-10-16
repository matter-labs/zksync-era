use futures::{future, FutureExt};
use governor::{
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use jsonrpc_core::{
    middleware::{self, Middleware},
    Error, FutureResponse, Request, Response, Version,
};
use jsonrpc_pubsub::Session;
use vise::{Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics};

use std::{future::Future, num::NonZeroU32, sync::Arc};

/// Configures the rate limiting for the WebSocket API.
/// Rate limiting is applied per active connection, e.g. a single connected user may not send more than X requests
/// per minute.
#[derive(Debug, Clone)]
pub struct RateLimitMetadata<T> {
    meta: T,
    rate_limiter: Option<Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>>>,
}

impl<T> RateLimitMetadata<T> {
    pub(crate) fn new(requests_per_minute: Option<u32>, meta: T) -> Self {
        let rate_limiter = if let Some(requests_per_minute) = requests_per_minute {
            assert!(requests_per_minute > 0, "requests_per_minute must be > 0");

            Some(Arc::new(RateLimiter::direct(Quota::per_minute(
                NonZeroU32::new(requests_per_minute).unwrap(),
            ))))
        } else {
            None
        };

        Self { meta, rate_limiter }
    }
}

impl<T: jsonrpc_core::Metadata> jsonrpc_core::Metadata for RateLimitMetadata<T> {}

impl<T: jsonrpc_pubsub::PubSubMetadata> jsonrpc_pubsub::PubSubMetadata for RateLimitMetadata<T> {
    fn session(&self) -> Option<Arc<Session>> {
        self.meta.session()
    }
}

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

/// Middleware that implements limiting for WebSocket connections:
/// - Limits the number of requests per minute for a single connection.
/// - Limits the maximum size of the batch requests.
///
/// Rate limiting data is stored in the metadata of the connection, while the maximum batch size is stored in the
/// middleware itself.
#[derive(Debug)]
pub(crate) struct LimitMiddleware {
    transport: Transport,
    max_batch_size: Option<usize>,
}

impl LimitMiddleware {
    pub fn new(transport: Transport, max_batch_size: Option<usize>) -> Self {
        Self {
            transport,
            max_batch_size,
        }
    }
}

impl<T: jsonrpc_core::Metadata> Middleware<RateLimitMetadata<T>> for LimitMiddleware {
    type Future = FutureResponse;

    type CallFuture = middleware::NoopCallFuture;

    fn on_request<F, X>(
        &self,
        request: Request,
        meta: RateLimitMetadata<T>,
        next: F,
    ) -> future::Either<Self::Future, X>
    where
        F: Fn(Request, RateLimitMetadata<T>) -> X + Send + Sync,
        X: Future<Output = Option<Response>> + Send + 'static,
    {
        // Check whether rate limiting is enabled, and if so, whether we should discard the request.
        // Note that RPC batch requests are stil counted as a single request.
        if let Some(rate_limiter) = &meta.rate_limiter {
            // Check number of actual RPC requests.
            let num_requests: usize = match &request {
                Request::Single(_) => 1,
                Request::Batch(batch) => batch.len(),
            };
            let num_requests = NonZeroU32::new(num_requests.max(1) as u32).unwrap();

            // Note: if required, we can extract data on rate limiting from the error.
            if rate_limiter.check_n(num_requests).is_err() {
                METRICS.rate_limited[&self.transport].inc();
                let err = Error {
                    code: jsonrpc_core::error::ErrorCode::ServerError(429),
                    message: "Too many requests".to_string(),
                    data: None,
                };

                let response = Response::from(err, Some(Version::V2));
                return future::ready(Some(response)).boxed().left_future();
            }
        }

        // Check whether the batch size is within the allowed limits.
        if let Request::Batch(batch) = &request {
            METRICS.size[&self.transport].observe(batch.len());

            if Some(batch.len()) > self.max_batch_size {
                METRICS.rejected[&self.transport].inc();
                let response = Response::from(Error::invalid_request(), Some(Version::V2));
                return future::ready(Some(response)).boxed().left_future();
            }
        }

        // Proceed with the request.
        next(request, meta).right_future()
    }
}
