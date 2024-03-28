//! Tests for `L2Client` focused on rate limiting.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use assert_matches::assert_matches;
use futures::future;
use jsonrpsee::{
    http_client::transport,
    types::{error::ErrorCode, ErrorObject},
};
use rand::{rngs::StdRng, Rng, SeedableRng};
use test_casing::test_casing;

use super::{
    metrics::{HttpErrorLabels, RequestLabels, RpcErrorLabels},
    *,
};

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

#[derive(Debug, Clone)]
struct MockClient;

#[async_trait]
impl ClientT for MockClient {
    async fn notification<Params>(&self, _method: &str, _params: Params) -> Result<(), Error>
    where
        Params: ToRpcParams + Send,
    {
        unreachable!("never called")
    }

    async fn request<R, Params>(&self, method: &str, _params: Params) -> Result<R, Error>
    where
        R: DeserializeOwned,
        Params: ToRpcParams + Send,
    {
        match method {
            "ok" => Ok(serde_json::from_value(serde_json::json!("ok"))?),
            "slow" => {
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok(serde_json::from_value(serde_json::json!("slow"))?)
            }
            "rate_limit" => {
                let http_err = transport::Error::RequestFailure { status_code: 429 };
                Err(Error::Transport(http_err.into()))
            }
            "eth_getBlockNumber" => Ok(serde_json::from_value(serde_json::json!("0x1"))?),
            _ => {
                let unknown_method_err =
                    ErrorObject::borrowed(ErrorCode::MethodNotFound.code(), "oops", None);
                Err(Error::Call(unknown_method_err))
            }
        }
    }

    async fn batch_request<'a, R>(
        &self,
        batch: BatchRequestBuilder<'a>,
    ) -> Result<BatchResponse<'a, R>, Error>
    where
        R: DeserializeOwned + fmt::Debug + 'a,
    {
        let request_handlers = batch
            .into_iter()
            .map(|(method, _)| self.request::<serde_json::Value, _>(method, [()]));
        let response_results = future::join_all(request_handlers).await;
        let mut responses = vec![];
        let mut successful_calls = 0;
        let mut failed_calls = 0;
        for result in response_results {
            match result {
                Ok(value) => {
                    responses.push(Ok(serde_json::from_value(value)?));
                    successful_calls += 1;
                }
                Err(Error::Call(err)) => {
                    responses.push(Err(err));
                    failed_calls += 1;
                }
                Err(err) => return Err(err),
            }
        }
        Ok(BatchResponse::new(
            successful_calls,
            responses,
            failed_calls,
        ))
    }
}

#[async_trait]
impl SubscriptionClientT for MockClient {
    async fn subscribe<'a, Notif, Params>(
        &self,
        _subscribe_method: &'a str,
        _params: Params,
        _unsubscribe_method: &'a str,
    ) -> Result<Subscription<Notif>, Error>
    where
        Params: ToRpcParams + Send,
        Notif: DeserializeOwned,
    {
        unreachable!("never called")
    }

    async fn subscribe_to_method<'a, Notif>(
        &self,
        _method: &'a str,
    ) -> Result<Subscription<Notif>, Error>
    where
        Notif: DeserializeOwned,
    {
        unreachable!("never called")
    }
}

#[tokio::test]
async fn wrapping_mock_client() {
    tokio::time::pause();

    let mut client = L2ClientBuilder::new(MockClient)
        .with_allowed_requests_per_second(NonZeroUsize::new(100).unwrap())
        .build()
        .for_component("test");
    let metrics = &*Box::leak(Box::default());
    client.metrics = metrics;
    assert_eq!(
        client.rate_limit.rate_limit_window,
        Duration::from_millis(50)
    );
    assert_eq!(client.rate_limit.rate_limit, 5);

    // Check that expected results are passed from the wrapped client.
    for _ in 0..10 {
        let output: String = client.request("ok", [()]).await.unwrap();
        assert_eq!(output, "ok");
    }

    let mut batch_request = BatchRequestBuilder::new();
    for _ in 0..5 {
        batch_request.insert("ok", [()]).unwrap();
    }
    batch_request.insert("slow", [()]).unwrap();
    client.batch_request::<String>(batch_request).await.unwrap();

    // Check that the batch hit the rate limit.
    let labels = RequestLabels {
        component: "test",
        method: "ok".to_string(),
    };
    assert!(metrics.rate_limit_latency.contains(&labels), "{metrics:?}");
    let labels = RequestLabels {
        component: "test",
        method: "slow".to_string(),
    };
    assert!(metrics.rate_limit_latency.contains(&labels), "{metrics:?}");

    // Check error reporting.
    let err = client
        .request::<String, _>("unknown", [()])
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(_));
    let labels = RpcErrorLabels {
        component: "test",
        method: "unknown".to_string(),
        code: ErrorCode::MethodNotFound.code(),
    };
    assert!(metrics.rpc_errors.contains(&labels), "{metrics:?}");

    let err = client
        .request::<String, _>("rate_limit", [()])
        .await
        .unwrap_err();
    assert_matches!(err, Error::Transport(_));
    let labels = HttpErrorLabels {
        component: "test",
        method: "rate_limit".to_string(),
        status: Some(429),
    };
    assert!(metrics.http_errors.contains(&labels), "{metrics:?}");
}
