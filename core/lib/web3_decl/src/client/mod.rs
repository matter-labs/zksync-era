//! L2 HTTP client.

use std::{
    fmt,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use jsonrpsee::{
    core::{
        client::{BatchResponse, ClientT, Error, Subscription, SubscriptionClientT},
        params::BatchRequestBuilder,
        traits::ToRpcParams,
    },
    http_client::{HttpClient, HttpClientBuilder},
};
use serde::de::DeserializeOwned;
use tokio::time::Instant;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy)]
enum RateLimitOrigin<'a> {
    Notification(&'a str),
    Request(&'a str),
    BatchRequest(&'a BatchRequestBuilder<'a>),
    Subscription(&'a str),
}

impl RateLimitOrigin<'_> {
    fn request_count(self) -> usize {
        match self {
            Self::BatchRequest(batch) => batch.iter().count(),
            _ => 1,
        }
    }
}

impl fmt::Display for RateLimitOrigin<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Notification(name) => write!(formatter, "notification `{name}`"),
            Self::Request(name) => write!(formatter, "request `{name}`"),
            Self::BatchRequest(req) => {
                let method_names: Vec<_> = req.iter().map(|(name, _)| name).collect();
                write!(formatter, "batch request {method_names:?}")
            }
            Self::Subscription(name) => write!(formatter, "subscription `{name}`"),
        }
    }
}

/// JSON-RPC client for the main node with built-in middleware support.
///
/// The client should be used instead of `HttpClient` etc. A single instance of the client should be built
/// and shared among all tasks run by the node in order to correctly rate-limit requests.
#[derive(Debug, Clone)]
pub struct L2Client {
    inner: HttpClient,
    rate_limit: SharedRateLimit,
    component_name: &'static str,
}

impl L2Client {
    pub fn builder() -> L2ClientBuilder {
        L2ClientBuilder::default()
    }

    /// Sets the component operating this client. This is used in logging etc.
    pub fn for_component(mut self, component_name: &'static str) -> Self {
        self.component_name = component_name;
        self
    }

    async fn limit_rate(&self, origin: RateLimitOrigin<'_>) {
        let stats = self.rate_limit.acquire(origin.request_count()).await;
        if !stats.was_waiting {
            return;
        }

        tracing::debug!(
            component = self.component_name,
            %origin,
            "Request to {origin} by component `{}` was rate-limited using policy {} reqs/{:?}: {stats:?}",
            self.component_name,
            self.rate_limit.rate_limit,
            self.rate_limit.rate_limit_window
        );
    }
}

#[async_trait]
impl ClientT for L2Client {
    async fn notification<Params>(&self, method: &str, params: Params) -> Result<(), Error>
    where
        Params: ToRpcParams + Send,
    {
        self.limit_rate(RateLimitOrigin::Notification(method)).await;
        self.inner.notification(method, params).await
    }

    async fn request<R, Params>(&self, method: &str, params: Params) -> Result<R, Error>
    where
        R: DeserializeOwned,
        Params: ToRpcParams + Send,
    {
        self.limit_rate(RateLimitOrigin::Request(method)).await;
        self.inner.request(method, params).await
    }

    async fn batch_request<'a, R>(
        &self,
        batch: BatchRequestBuilder<'a>,
    ) -> Result<BatchResponse<'a, R>, Error>
    where
        R: DeserializeOwned + fmt::Debug + 'a,
    {
        self.limit_rate(RateLimitOrigin::BatchRequest(&batch)).await;
        self.inner.batch_request(batch).await
    }
}

#[async_trait]
impl SubscriptionClientT for L2Client {
    async fn subscribe<'a, Notif, Params>(
        &self,
        subscribe_method: &'a str,
        params: Params,
        unsubscribe_method: &'a str,
    ) -> Result<Subscription<Notif>, Error>
    where
        Params: ToRpcParams + Send,
        Notif: DeserializeOwned,
    {
        self.limit_rate(RateLimitOrigin::Subscription(subscribe_method))
            .await;
        self.inner
            .subscribe(subscribe_method, params, unsubscribe_method)
            .await
    }

    async fn subscribe_to_method<'a, Notif>(
        &self,
        method: &'a str,
    ) -> Result<Subscription<Notif>, Error>
    where
        Notif: DeserializeOwned,
    {
        self.limit_rate(RateLimitOrigin::Subscription(method)).await;
        self.inner.subscribe_to_method(method).await
    }
}

/// Builder for the [`L2Client`].
#[derive(Debug)]
pub struct L2ClientBuilder {
    rate_limit: (usize, Duration),
}

impl Default for L2ClientBuilder {
    fn default() -> Self {
        Self {
            rate_limit: (1, Duration::ZERO),
        }
    }
}

impl L2ClientBuilder {
    /// Sets the rate limit for the client. The rate limit is applied across all client instances,
    /// including cloned ones.
    pub fn with_allowed_requests_per_second(mut self, rps: NonZeroUsize) -> Self {
        let rps = usize::from(rps);
        // Define the rate limiting window to be sufficiently small so that we don't get stampeding requests.
        self.rate_limit = match rps {
            1..=24 => (1, Duration::from_secs(1) / rps as u32),
            // Round requests per window up if necessary. The relative error of this rounding is selected to be <20%
            // in all cases.
            25..=49 => (rps.div_ceil(5), Duration::from_millis(200)),
            50..=99 => (rps.div_ceil(10), Duration::from_millis(100)),
            _ => (rps.div_ceil(20), Duration::from_millis(50)),
        };
        self
    }

    /// Builds the client.
    pub fn build(self, url: &str) -> anyhow::Result<L2Client> {
        tracing::info!(
            "Creating HTTP JSON-RPC client with URL: {url} and rate limit: {:?}",
            self.rate_limit
        );
        let inner = HttpClientBuilder::default().build(url)?;
        Ok(L2Client {
            inner,
            rate_limit: SharedRateLimit::new(self.rate_limit.0, self.rate_limit.1),
            component_name: "",
        })
    }
}

#[derive(Debug, Default)]
#[must_use = "stats should be reported"]
struct AcquireStats {
    was_waiting: bool,
    total_sleep_time: Duration,
}

#[derive(Debug)]
enum SharedRateLimitState {
    Limited {
        until: Instant,
    },
    Ready {
        until: Instant,
        remaining_requests: usize,
    },
}

#[derive(Debug)]
struct SharedRateLimit {
    rate_limit: usize,
    rate_limit_window: Duration,
    state: Arc<Mutex<SharedRateLimitState>>,
}

impl Clone for SharedRateLimit {
    fn clone(&self) -> Self {
        Self {
            rate_limit: self.rate_limit,
            rate_limit_window: self.rate_limit_window,
            state: self.state.clone(),
        }
    }
}

impl SharedRateLimit {
    fn new(rate_limit: usize, rate_limit_window: Duration) -> Self {
        let state = Arc::new(Mutex::new(SharedRateLimitState::Ready {
            until: Instant::now(),
            remaining_requests: rate_limit,
        }));
        Self {
            rate_limit,
            rate_limit_window,
            state,
        }
    }

    /// Acquires the specified number of requests waiting if necessary. If the number of requests exceeds
    /// the capacity of the limiter, it is saturated (i.e., this method cannot hang indefinitely or panic
    /// in this case).
    ///
    /// This implementation is similar to [`RateLimit`] middleware in Tower, but is shared among client instances.
    ///
    /// [`RateLimit`]: https://docs.rs/tower/latest/tower/limit/struct.RateLimit.html
    async fn acquire(&self, request_count: usize) -> AcquireStats {
        let mut stats = AcquireStats::default();
        let mut state = loop {
            // A separate scope is required to not hold a mutex guard across the `await` point,
            // which is not only semantically incorrect, but also makes the future `!Send` (unfortunately,
            // async Rust doesn't seem to understand non-lexical lifetimes).
            let now = Instant::now();
            let until = {
                let mut state = self.state.lock().expect("state is poisoned");
                match &*state {
                    SharedRateLimitState::Ready { .. } => break state,
                    SharedRateLimitState::Limited { until } if *until <= now => {
                        // At this point, local time is `>= until`; thus, the state should be reset.
                        // Because we hold an exclusive lock on `state`, there's no risk of a data race.
                        *state = SharedRateLimitState::Ready {
                            until: now + self.rate_limit_window,
                            remaining_requests: self.rate_limit,
                        };
                        break state;
                    }
                    SharedRateLimitState::Limited { until } => *until,
                }
            };

            stats.was_waiting = true;
            stats.total_sleep_time += until.duration_since(now);
            tokio::time::sleep_until(until).await;
        };
        let SharedRateLimitState::Ready {
            until,
            remaining_requests,
        } = &mut *state
        else {
            unreachable!();
        };

        let now = Instant::now();
        // Reset the period if it has elapsed.
        if now >= *until {
            *until = now + self.rate_limit_window;
            *remaining_requests = self.rate_limit;
        }
        *remaining_requests = remaining_requests.saturating_sub(request_count);
        if *remaining_requests == 0 {
            *state = SharedRateLimitState::Limited { until: *until };
        }
        stats
    }
}
