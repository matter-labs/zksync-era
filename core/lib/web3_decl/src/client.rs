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
    pub fn with_rate_limit(mut self, count: NonZeroUsize, per: Duration) -> Self {
        self.rate_limit = (count.into(), per);
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

#[cfg(test)]
mod tests {
    use futures::future;
    use rand::{rngs::StdRng, Rng, SeedableRng};
    use test_casing::test_casing;

    use super::*;

    #[derive(Debug, Clone, Default)]
    struct MockService(Arc<Mutex<Vec<Instant>>>);

    async fn poll_service(limiter: &SharedRateLimit, service: &MockService) {
        let _ = limiter.acquire(1).await;
        service.0.lock().unwrap().push(Instant::now());
    }

    #[test_casing(3, [1, 2, 3])]
    #[tokio::test]
    async fn rate_limiting_with_single_instance(rate_limit: usize) {
        tokio::time::pause();

        let service = MockService::default();
        let limiter = SharedRateLimit::new(rate_limit, Duration::from_secs(1));
        for _ in 0..10 {
            poll_service(&limiter, &service).await;
        }

        let timestamps = service.0.lock().unwrap().clone();
        assert_eq!(timestamps.len(), 10);
        assert_timestamps_spacing_with_mock_clock(&timestamps, rate_limit);
    }

    #[tokio::test]
    async fn rate_limiting_resetting_state() {
        tokio::time::pause();

        let service = MockService::default();
        let limiter = SharedRateLimit::new(2, Duration::from_secs(1));
        poll_service(&limiter, &service).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        poll_service(&limiter, &service).await;
        poll_service(&limiter, &service).await; // should wait for the rate limit window to reset
        poll_service(&limiter, &service).await;

        let timestamps = service.0.lock().unwrap().clone();
        assert_eq!(timestamps.len(), 4);
        let diffs = timestamp_diffs(&timestamps);
        assert_eq!(
            diffs,
            [
                Duration::from_millis(301),
                Duration::from_millis(700),
                Duration::ZERO
            ]
        );
    }

    #[tokio::test]
    async fn no_op_rate_limiting() {
        tokio::time::pause();

        let service = MockService::default();
        let limiter = SharedRateLimit::new(1, Duration::ZERO);
        for _ in 0..10 {
            poll_service(&limiter, &service).await;
        }

        let timestamps = service.0.lock().unwrap().clone();
        assert_eq!(timestamps.len(), 10);
        let diffs = timestamp_diffs(&timestamps);
        assert_eq!(diffs, [Duration::ZERO; 9]);
    }

    fn timestamp_diffs(timestamps: &[Instant]) -> Vec<Duration> {
        let diffs = timestamps.windows(2).map(|window| match window {
            [prev, next] => *next - *prev,
            _ => unreachable!(),
        });
        diffs.collect()
    }

    fn assert_timestamps_spacing_with_mock_clock(timestamps: &[Instant], rate_limit: usize) {
        let diffs = timestamp_diffs(timestamps);

        // Since we use a mock clock, `diffs` should be deterministic.
        for (i, &diff) in diffs.iter().enumerate() {
            if i % rate_limit == rate_limit - 1 {
                assert!(diff > Duration::from_secs(1), "{diffs:?}");
            } else {
                assert_eq!(diff, Duration::ZERO, "{diffs:?}");
            }
        }
    }

    #[test_casing(3, [2, 3, 5])]
    #[tokio::test]
    async fn rate_limiting_with_multiple_instances(rate_limit: usize) {
        tokio::time::pause();

        let service = MockService::default();
        let limiter = SharedRateLimit::new(rate_limit, Duration::from_secs(1));
        let calls = (0..50).map(|_| {
            let service = service.clone();
            let limiter = limiter.clone();
            async move {
                poll_service(&limiter, &service).await;
            }
        });
        future::join_all(calls).await;

        let timestamps = service.0.lock().unwrap().clone();
        assert_eq!(timestamps.len(), 50);
        assert_timestamps_spacing_with_mock_clock(&timestamps, rate_limit);
    }

    async fn test_rate_limiting_with_rng(rate_limit: usize, rng_seed: u64) {
        const RATE_LIMIT_WINDOW_MS: u64 = 50;

        let mut rng = StdRng::seed_from_u64(rng_seed);
        let service = MockService::default();
        let rate_limit_window = Duration::from_millis(RATE_LIMIT_WINDOW_MS);
        let limiter = SharedRateLimit::new(rate_limit, rate_limit_window);
        let max_sleep_duration_ms = RATE_LIMIT_WINDOW_MS * 2 / rate_limit as u64;

        let mut call_tasks = vec![];
        for _ in 0..50 {
            let sleep_duration_ms = rng.gen_range(0..=max_sleep_duration_ms);
            tokio::time::sleep(Duration::from_millis(sleep_duration_ms)).await;

            let service = service.clone();
            let limiter = limiter.clone();
            call_tasks.push(tokio::spawn(async move {
                poll_service(&limiter, &service).await;
            }));
        }
        future::try_join_all(call_tasks).await.unwrap();

        let timestamps = service.0.lock().unwrap().clone();
        assert_eq!(timestamps.len(), 50);
        let mut window_start = (0, timestamps[0]);
        // Add an artificial terminal timestamp to check the last rate limiting window.
        let it = timestamps
            .iter()
            .copied()
            .chain([Instant::now() + rate_limit_window])
            .enumerate()
            .skip(1);
        for (i, timestamp) in it {
            if timestamp - window_start.1 >= rate_limit_window {
                assert!(
                    i - window_start.0 <= rate_limit,
                    "diffs={:?}, idx={i}, window_start={window_start:?}",
                    timestamp_diffs(&timestamps)
                );
                window_start = (i, timestamp);
            }
        }
    }

    #[test_casing(4, [2, 3, 5, 8])]
    #[tokio::test]
    async fn rate_limiting_with_rng(rate_limit: usize) {
        tokio::time::pause();

        for rng_seed in 0..1_000 {
            println!("Testing RNG seed: {rng_seed}");
            test_rate_limiting_with_rng(rate_limit, rng_seed).await;
        }
    }

    #[test_casing(4, [2, 3, 5, 8])]
    #[tokio::test(flavor = "multi_thread")]
    async fn rate_limiting_with_rng_and_threads(rate_limit: usize) {
        const RNG_SEED: u64 = 123;

        test_rate_limiting_with_rng(rate_limit, RNG_SEED).await;
    }
}
