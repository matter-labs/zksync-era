use std::num::NonZeroU32;
use std::sync::Arc;

use governor::clock::DefaultClock;
use governor::middleware::NoOpMiddleware;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};

use jsonrpc_core::middleware::Middleware;
use jsonrpc_core::*;
use jsonrpc_pubsub::Session;

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

#[derive(Debug, Clone, Copy)]
pub(crate) enum Transport {
    Ws,
}

impl Transport {
    pub fn as_str(&self) -> &'static str {
        match self {
            Transport::Ws => "ws",
        }
    }
}

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
        request: jsonrpc_core::Request,
        meta: RateLimitMetadata<T>,
        next: F,
    ) -> futures::future::Either<Self::Future, X>
    where
        F: Fn(jsonrpc_core::Request, RateLimitMetadata<T>) -> X + Send + Sync,
        X: futures::Future<Output = Option<jsonrpc_core::Response>> + Send + 'static,
    {
        // Check whether rate limiting is enabled, and if so, whether we should discard the request.
        // Note that RPC batch requests are stil counted as a single request.
        if let Some(rate_limiter) = &meta.rate_limiter {
            // Check number of actual RPC requests.
            let num_requests: usize = match &request {
                jsonrpc_core::Request::Single(_) => 1,
                jsonrpc_core::Request::Batch(batch) => batch.len(),
            };
            let num_requests = NonZeroU32::new(num_requests.max(1) as u32).unwrap();

            // Note: if required, we can extract data on rate limiting from the error.
            if rate_limiter.check_n(num_requests).is_err() {
                metrics::increment_counter!("api.jsonrpc_backend.batch.rate_limited", "transport" => self.transport.as_str());
                let err = jsonrpc_core::error::Error {
                    code: jsonrpc_core::error::ErrorCode::ServerError(429),
                    message: "Too many requests".to_string(),
                    data: None,
                };

                return futures::future::Either::Left(Box::pin(futures::future::ready(Some(
                    jsonrpc_core::Response::from(err, Some(Version::V2)),
                ))));
            }
        }

        // Check whether the batch size is within the allowed limits.
        if let jsonrpc_core::Request::Batch(batch) = &request {
            metrics::histogram!("api.jsonrpc_backend.batch.size", batch.len() as f64, "transport" => self.transport.as_str());

            if Some(batch.len()) > self.max_batch_size {
                metrics::increment_counter!("api.jsonrpc_backend.batch.rejected", "transport" => self.transport.as_str());
                return futures::future::Either::Left(Box::pin(futures::future::ready(Some(
                    jsonrpc_core::Response::from(Error::invalid_request(), Some(Version::V2)),
                ))));
            }
        }

        // Proceed with the request.
        futures::future::Either::Right(next(request, meta))
    }
}
