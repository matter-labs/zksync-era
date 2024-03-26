use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{ready, Context, Poll},
    time::Duration,
};

use tokio::time::{Instant, Sleep};
use tower::Service;
use zksync_web3_decl::jsonrpsee::http_client::{
    transport::HttpBackend, HttpClient, HttpClientBuilder,
};

type HttpClientLayers = SharedRateLimit<HttpBackend>;

#[derive(Debug, Clone)]
pub struct L2Client(HttpClient<HttpClientLayers>);

impl L2Client {
    pub async fn new(url: &str, rate_limit_per_second: usize) -> anyhow::Result<Self> {
        assert!(rate_limit_per_second > 0);

        let service_builder = tower::ServiceBuilder::new().layer_fn(move |service| {
            SharedRateLimit::new(service, rate_limit_per_second, Duration::from_secs(1))
        });
        let inner = HttpClientBuilder::default()
            .set_http_middleware(service_builder)
            .build(url)?;
        Ok(Self(inner))
    }
}

#[derive(Debug)]
enum SharedRateLimitState {
    Limited { until: Instant },
    Ready { until: Instant, permits: usize },
}

#[derive(Debug)]
pub struct SharedRateLimit<S> {
    inner: S,
    rate_limit: usize,
    rate_limit_window: Duration,
    state: Arc<Mutex<SharedRateLimitState>>,
    sleep: Pin<Box<Sleep>>,
}

impl<S: Clone> Clone for SharedRateLimit<S> {
    fn clone(&self) -> Self {
        let now = Instant::now();
        Self {
            inner: self.inner.clone(),
            rate_limit: self.rate_limit,
            rate_limit_window: self.rate_limit_window,
            state: self.state.clone(),
            sleep: Box::pin(tokio::time::sleep_until(now)),
        }
    }
}

impl<S> SharedRateLimit<S> {
    fn new(service: S, rate_limit: usize, rate_limit_window: Duration) -> Self {
        let now = Instant::now();
        let state = Arc::new(Mutex::new(SharedRateLimitState::Ready {
            until: now,
            permits: rate_limit,
        }));
        Self {
            inner: service,
            rate_limit,
            rate_limit_window,
            state,
            sleep: Box::pin(tokio::time::sleep_until(now)),
        }
    }
}

impl<Req, S: Service<Req>> Service<Req> for SharedRateLimit<S> {
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let mut state = self.state.lock().expect("state is poisoned");
        match &*state {
            SharedRateLimitState::Ready { .. } => {
                return Poll::Ready(ready!(self.inner.poll_ready(cx)))
            }
            SharedRateLimitState::Limited { until } => {
                self.sleep.as_mut().reset(*until);
                ready!(self.sleep.as_mut().poll(cx));
                // At this point, local time is `>= until`; thus, the state should be reset.
                *state = SharedRateLimitState::Ready {
                    until: Instant::now() + self.rate_limit_window,
                    permits: self.rate_limit,
                };
            }
        }

        drop(state);
        Poll::Ready(ready!(self.inner.poll_ready(cx)))
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let mut state = self.state.lock().expect("state is poisoned");
        let SharedRateLimitState::Ready { until, permits } = &mut *state else {
            panic!("service is not ready; Service::poll_ready should have been called first");
        };
        let now = Instant::now();
        // Reset the period if it has elapsed.
        if now >= *until {
            *until = now + self.rate_limit_window;
            *permits = self.rate_limit;
        }

        if *permits > 1 {
            *permits -= 1;
        } else {
            *state = SharedRateLimitState::Limited { until: *until };
        }
        drop(state);

        self.inner.call(req)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        convert::Infallible,
        future::{ready, Ready},
    };

    use futures::future;
    use rand::{thread_rng, Rng};
    use test_casing::test_casing;

    use super::*;

    #[derive(Debug, Clone, Default)]
    struct MockService(Arc<Mutex<Vec<Instant>>>);

    impl Service<()> for MockService {
        type Response = ();
        type Error = Infallible;
        type Future = Ready<Result<(), Infallible>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: ()) -> Self::Future {
            self.0.lock().expect("poisoned").push(Instant::now());
            ready(Ok(()))
        }
    }

    async fn poll_service(service: &mut SharedRateLimit<MockService>) {
        future::poll_fn(|cx| service.poll_ready(cx)).await.unwrap();
        service.call(()).await.unwrap();
    }

    #[test_casing(3, [1, 2, 3])]
    #[tokio::test]
    async fn rate_limiting_with_single_instance(rate_limit: usize) {
        tokio::time::pause();

        let service = MockService::default();
        let mut service = SharedRateLimit::new(service, rate_limit, Duration::from_secs(1));
        for _ in 0..10 {
            poll_service(&mut service).await;
        }

        let timestamps = service.inner.0.lock().unwrap().clone();
        assert_eq!(timestamps.len(), 10);
        assert_timestamps_spacing_with_mock_clock(&timestamps, rate_limit);
    }

    #[tokio::test]
    async fn rate_limiting_resetting_state() {
        tokio::time::pause();

        let service = MockService::default();
        let mut service = SharedRateLimit::new(service, 2, Duration::from_secs(1));
        poll_service(&mut service).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        poll_service(&mut service).await;
        poll_service(&mut service).await; // should wait for the rate limit window to reset
        poll_service(&mut service).await;

        let timestamps = service.inner.0.lock().unwrap().clone();
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
        let service = SharedRateLimit::new(service, rate_limit, Duration::from_secs(1));
        let calls = (0..50).map(|_| {
            let mut service = service.clone();
            async move {
                poll_service(&mut service).await;
            }
        });
        future::join_all(calls).await;

        let timestamps = service.inner.0.lock().unwrap().clone();
        assert_eq!(timestamps.len(), 50);
        assert_timestamps_spacing_with_mock_clock(&timestamps, rate_limit);
    }

    #[test_casing(3, [2, 3, 5])]
    #[tokio::test(flavor = "multi_thread")]
    async fn rate_limiting_with_multiple_instances_and_threads(rate_limit: usize) {
        let rate_limit_window = Duration::from_millis(50);
        let service = MockService::default();
        let service = SharedRateLimit::new(service, rate_limit, rate_limit_window);
        let calls = (0..50).map(|_| {
            let mut service = service.clone();
            tokio::spawn(async move {
                let sleep_duration = thread_rng().gen_range(0..20);
                tokio::time::sleep(Duration::from_millis(sleep_duration)).await;
                poll_service(&mut service).await;
            })
        });
        future::try_join_all(calls).await.unwrap();

        let timestamps = service.inner.0.lock().unwrap().clone();
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
