use std::{any, fmt, future::Future, time::Duration};

use async_trait::async_trait;
use rand::Rng;

use crate::{
    metrics::OBJECT_STORE_METRICS,
    raw::{Bucket, ObjectStore, ObjectStoreError},
};

/// Information about request added to logs.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // fields are used via `Debug` impl in logs
enum Request<'a> {
    New,
    Get(Bucket, &'a str),
    Put(Bucket, &'a str),
    Remove(Bucket, &'a str),
}

impl Request<'_> {
    #[tracing::instrument(
        name = "object_store::Request::retry",
        skip(f), // output request and store as a part of structured logs
        fields(retries) // Will be recorded before returning from the function
    )]
    async fn retry<T, Fut, F>(
        self,
        store: &impl fmt::Debug,
        max_retries: u16,
        mut f: F,
    ) -> Result<T, ObjectStoreError>
    where
        Fut: Future<Output = Result<T, ObjectStoreError>>,
        F: FnMut() -> Fut,
    {
        let mut retries = 1;
        let mut backoff_secs = 1;
        let result = loop {
            match f().await {
                Ok(result) => break Ok(result),
                Err(err) if err.is_retriable() => {
                    if retries > max_retries {
                        tracing::warn!(?err, "Exhausted {max_retries} retries performing request; returning last error");
                        break Err(err);
                    }
                    tracing::info!(?err, "Failed request, retries: {retries}/{max_retries}");
                    retries += 1;
                    // Randomize sleep duration to prevent stampeding the server if multiple requests are initiated at the same time.
                    let sleep_duration = Duration::from_secs(backoff_secs)
                        .mul_f32(rand::thread_rng().gen_range(0.8..1.2));
                    tokio::time::sleep(sleep_duration).await;
                    backoff_secs *= 2;
                }
                Err(err) => {
                    if !err.to_string().contains("key not found") {
                        tracing::warn!(%err, "Failed request with a fatal error");
                    }
                    break Err(err);
                }
            }
        };
        tracing::Span::current().record("retries", retries);
        result
    }
}

/// [`ObjectStore`] wrapper that retries all operations according to a reasonable policy.
#[derive(Debug)]
pub(crate) struct StoreWithRetries<S> {
    inner: S,
    max_retries: u16,
}

impl<S: ObjectStore> StoreWithRetries<S> {
    /// Creates a store based on the provided async initialization closure.
    pub async fn try_new<Fut>(
        max_retries: u16,
        init_fn: impl FnMut() -> Fut,
    ) -> Result<Self, ObjectStoreError>
    where
        Fut: Future<Output = Result<S, ObjectStoreError>>,
    {
        Ok(Self {
            inner: Request::New
                .retry(&any::type_name::<S>(), max_retries, init_fn)
                .await?,
            max_retries,
        })
    }
}

// Object store metrics are placed here because `ObjectStoreFactory` (which in practice produces "production" object stores)
// wraps stores in `StoreWithRetries`. If this is changed, metrics will need to be refactored correspondingly.
#[async_trait]
impl<S: ObjectStore> ObjectStore for StoreWithRetries<S> {
    async fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        let latency = OBJECT_STORE_METRICS.start_fetch(bucket);
        let result = Request::Get(bucket, key)
            .retry(&self.inner, self.max_retries, || {
                self.inner.get_raw(bucket, key)
            })
            .await;
        latency.observe();
        result
    }

    async fn put_raw(
        &self,
        bucket: Bucket,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), ObjectStoreError> {
        let latency = OBJECT_STORE_METRICS.start_store(bucket);
        let result = Request::Put(bucket, key)
            .retry(&self.inner, self.max_retries, || {
                self.inner.put_raw(bucket, key, value.clone())
            })
            .await;
        latency.observe();
        result
    }

    async fn remove_raw(&self, bucket: Bucket, key: &str) -> Result<(), ObjectStoreError> {
        Request::Remove(bucket, key)
            .retry(&self.inner, self.max_retries, || {
                self.inner.remove_raw(bucket, key)
            })
            .await
    }

    fn storage_prefix_raw(&self, bucket: Bucket) -> String {
        self.inner.storage_prefix_raw(bucket)
    }
}

#[cfg(test)]
mod test {
    use std::sync::atomic::{AtomicU16, Ordering};

    use assert_matches::assert_matches;

    use super::*;

    fn retriable_error() -> ObjectStoreError {
        ObjectStoreError::Other {
            is_retriable: true,
            source: "oops".into(),
        }
    }

    #[tokio::test]
    async fn test_retry_success_immediate() {
        let result = Request::New
            .retry(&"store", 2, || async { Ok(42) })
            .await
            .unwrap();
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_retry_failure_exhausted() {
        let err = Request::New
            .retry(&"store", 2, || async { Err::<i32, _>(retriable_error()) })
            .await
            .unwrap_err();
        assert_matches!(err, ObjectStoreError::Other { .. });
    }

    async fn retry_success_after_n_retries(n: u16) -> Result<u32, ObjectStoreError> {
        let retries = AtomicU16::new(0);
        Request::New
            .retry(&"store", n, || async {
                let retries = retries.fetch_add(1, Ordering::Relaxed);
                if retries + 1 == n {
                    Ok(42)
                } else {
                    Err(retriable_error())
                }
            })
            .await
    }

    #[tokio::test]
    async fn test_retry_success_after_retry() {
        let result = Request::New
            .retry(&"store", 2, || retry_success_after_n_retries(2))
            .await
            .unwrap();
        assert_eq!(result, 42);
    }
}
