use std::{
    fmt::Debug,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use tokio::time::Instant;
use zksync_web3_decl::jsonrpsee::{
    core::{
        client::{BatchResponse, ClientT, Error, Subscription, SubscriptionClientT},
        params::BatchRequestBuilder,
        traits::ToRpcParams,
    },
    http_client::{HttpClient, HttpClientBuilder},
};

#[derive(Debug, Clone)]
pub struct L2Client {
    inner: HttpClient,
    rate_limit: Option<SharedRateLimit>,
}

impl L2Client {
    pub fn builder() -> L2ClientBuilder {
        L2ClientBuilder::default()
    }
}

#[async_trait]
impl ClientT for L2Client {
    async fn notification<Params>(&self, method: &str, params: Params) -> Result<(), Error>
    where
        Params: ToRpcParams + Send,
    {
        if let Some(rate_limit) = &self.rate_limit {
            rate_limit.ready(1).await;
        }
        self.inner.notification(method, params).await
    }

    async fn request<R, Params>(&self, method: &str, params: Params) -> Result<R, Error>
    where
        R: DeserializeOwned,
        Params: ToRpcParams + Send,
    {
        if let Some(rate_limit) = &self.rate_limit {
            rate_limit.ready(1).await;
        }
        self.inner.request(method, params).await
    }

    async fn batch_request<'a, R>(
        &self,
        batch: BatchRequestBuilder<'a>,
    ) -> Result<BatchResponse<'a, R>, Error>
    where
        R: DeserializeOwned + Debug + 'a,
    {
        if let Some(rate_limit) = &self.rate_limit {
            rate_limit.ready(batch.iter().count()).await;
        }
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
        if let Some(rate_limit) = &self.rate_limit {
            rate_limit.ready(1).await;
        }
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
        if let Some(rate_limit) = &self.rate_limit {
            rate_limit.ready(1).await;
        }
        self.inner.subscribe_to_method(method).await
    }
}

/// Builder for the [`L2Client`].
#[derive(Debug, Default)]
pub struct L2ClientBuilder {
    rate_limit: Option<(usize, Duration)>,
}

impl L2ClientBuilder {
    /// Sets the rate limit for the client. The rate limit is applied across all client instances,
    /// including cloned ones.
    pub fn with_rate_limit(mut self, count: usize, per: Duration) -> Self {
        assert!(count > 0, "Rate limit count must be positive");
        assert!(per > Duration::ZERO, "Rate limit window must be positive");
        self.rate_limit = Some((count, per));
        self
    }

    /// Builds the client.
    pub fn build(self, url: &str) -> anyhow::Result<L2Client> {
        let inner = HttpClientBuilder::default().build(url)?;
        Ok(L2Client {
            inner,
            rate_limit: self
                .rate_limit
                .map(|(count, per)| SharedRateLimit::new(count, per)),
        })
    }
}

#[derive(Debug)]
enum SharedRateLimitState {
    Limited { until: Instant },
    Ready { until: Instant, permits: usize },
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
            permits: rate_limit,
        }));
        Self {
            rate_limit,
            rate_limit_window,
            state,
        }
    }

    async fn ready(&self, permit_count: usize) {
        let mut state = loop {
            // A separate scope is required to not hold a mutex guard across the `await` point,
            // which is not only semantically incorrect, but also makes the future `!Send`.
            let until = {
                let mut state = self.state.lock().expect("state is poisoned");
                let now = Instant::now();
                match &*state {
                    SharedRateLimitState::Ready { .. } => break state,
                    SharedRateLimitState::Limited { until }
                        if until.duration_since(now) == Duration::ZERO =>
                    {
                        // At this point, local time is `>= until`; thus, the state should be reset.
                        // Because we hold an exclusive lock on `state`, there's no risk of a data race.
                        *state = SharedRateLimitState::Ready {
                            until: now + self.rate_limit_window,
                            permits: self.rate_limit,
                        };
                        break state;
                    }
                    SharedRateLimitState::Limited { until } => *until,
                }
            };
            tokio::time::sleep_until(until).await;
        };
        let SharedRateLimitState::Ready { until, permits } = &mut *state else {
            unreachable!();
        };

        let now = Instant::now();
        // Reset the period if it has elapsed.
        if now >= *until {
            *until = now + self.rate_limit_window;
            *permits = self.rate_limit;
        }
        *permits = permits.saturating_sub(permit_count);
        if *permits == 0 {
            *state = SharedRateLimitState::Limited { until: *until };
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::future;
    use rand::{thread_rng, Rng};
    use test_casing::test_casing;

    use super::*;

    #[derive(Debug, Clone, Default)]
    struct MockService(Arc<Mutex<Vec<Instant>>>);

    async fn poll_service(limiter: &SharedRateLimit, service: &MockService) {
        limiter.ready(1).await;
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

    #[test_casing(3, [2, 3, 5])]
    #[tokio::test(flavor = "multi_thread")]
    async fn rate_limiting_with_multiple_instances_and_threads(rate_limit: usize) {
        let rate_limit_window = Duration::from_millis(50);
        let service = MockService::default();
        let limiter = SharedRateLimit::new(rate_limit, rate_limit_window);
        let calls = (0..50).map(|_| {
            let service = service.clone();
            let limiter = limiter.clone();
            tokio::spawn(async move {
                let sleep_duration = thread_rng().gen_range(0..20);
                tokio::time::sleep(Duration::from_millis(sleep_duration)).await;
                poll_service(&limiter, &service).await;
            })
        });
        future::try_join_all(calls).await.unwrap();

        let timestamps = service.0.lock().unwrap().clone();
        assert_eq!(timestamps.len(), 50);
        for window in timestamps.windows(rate_limit + 1) {
            let first_timestamp = *window.first().unwrap();
            let last_timestamp = *window.last().unwrap();
            assert!(
                last_timestamp - first_timestamp >= rate_limit_window,
                "{:?}",
                timestamp_diffs(&timestamps)
            );
        }
    }
}
