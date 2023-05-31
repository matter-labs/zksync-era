//! GCS-based [`ObjectStore`] implementation.

use google_cloud_auth::{credentials::CredentialsFile, error::Error};
use google_cloud_default::WithAuthExt;
use google_cloud_storage::{
    client::{Client, ClientConfig},
    http::objects::{
        delete::DeleteObjectRequest,
        download::Range,
        get::GetObjectRequest,
        upload::{Media, UploadObjectRequest, UploadType},
    },
    http::Error as HttpError,
};
use http::StatusCode;
use tokio::runtime::{Handle, RuntimeFlavor};

use std::{
    fmt,
    future::Future,
    thread,
    time::{Duration, Instant},
};

use crate::raw::{Bucket, ObjectStore, ObjectStoreError};

async fn retry<T, E, Fut, F>(max_retries: u16, mut f: F) -> Result<T, E>
where
    Fut: Future<Output = Result<T, E>>,
    F: FnMut() -> Fut,
{
    let mut retries = 1;
    let mut backoff = 1;
    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(err) => {
                vlog::warn!("Failed gcs request {retries}/{max_retries}, retrying.");
                if retries > max_retries {
                    return Err(err);
                }
                retries += 1;
                tokio::time::sleep(Duration::from_secs(backoff)).await;
                backoff *= 2;
            }
        }
    }
}

pub struct AsyncGoogleCloudStorage {
    bucket_prefix: String,
    max_retries: u16,
    client: Client,
}

impl fmt::Debug for AsyncGoogleCloudStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoogleCloudStorageAsync")
            .field("bucket_prefix", &self.bucket_prefix)
            .field("max_retries", &self.max_retries)
            .finish()
    }
}

impl AsyncGoogleCloudStorage {
    pub async fn new(
        credential_file_path: Option<String>,
        bucket_prefix: String,
        max_retries: u16,
    ) -> Self {
        let client_config = retry(max_retries, || {
            Self::get_client_config(credential_file_path.clone())
        })
        .await
        .expect("failed fetching GCS client config after retries");

        Self {
            client: Client::new(client_config),
            bucket_prefix,
            max_retries,
        }
    }

    async fn get_client_config(
        credential_file_path: Option<String>,
    ) -> Result<ClientConfig, Error> {
        if let Some(path) = credential_file_path {
            let cred_file = CredentialsFile::new_from_file(path)
                .await
                .expect("failed loading GCS credential file");
            ClientConfig::default().with_credentials(cred_file).await
        } else {
            ClientConfig::default().with_auth().await
        }
    }

    fn filename(bucket: &str, filename: &str) -> String {
        format!("{bucket}/{filename}")
    }

    pub(crate) async fn get_async(
        &self,
        bucket: &'static str,
        key: &str,
    ) -> Result<Vec<u8>, ObjectStoreError> {
        let started_at = Instant::now();
        let filename = Self::filename(bucket, key);
        vlog::trace!(
            "Fetching data from GCS for key {filename} from bucket {}",
            self.bucket_prefix
        );

        let request = GetObjectRequest {
            bucket: self.bucket_prefix.clone(),
            object: filename,
            ..GetObjectRequest::default()
        };
        let range = Range::default();
        let blob = retry(self.max_retries, || {
            self.client.download_object(&request, &range)
        })
        .await;

        vlog::trace!(
            "Fetched data from GCS for key {key} from bucket {bucket} and it took: {:?}",
            started_at.elapsed()
        );
        metrics::histogram!(
            "server.object_store.fetching_time",
            started_at.elapsed(),
            "bucket" => bucket
        );
        blob.map_err(ObjectStoreError::from)
    }

    pub(crate) async fn put_async(
        &self,
        bucket: &'static str,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), ObjectStoreError> {
        let started_at = Instant::now();
        let filename = Self::filename(bucket, key);
        vlog::trace!(
            "Storing data to GCS for key {filename} from bucket {}",
            self.bucket_prefix
        );

        let upload_type = UploadType::Simple(Media::new(filename));
        let request = UploadObjectRequest {
            bucket: self.bucket_prefix.clone(),
            ..Default::default()
        };
        let object = retry(self.max_retries, || {
            self.client
                .upload_object(&request, value.clone(), &upload_type)
        })
        .await;

        vlog::trace!(
            "Stored data to GCS for key {key} from bucket {bucket} and it took: {:?}",
            started_at.elapsed()
        );
        metrics::histogram!(
            "server.object_store.storing_time",
            started_at.elapsed(),
            "bucket" => bucket
        );
        object.map(drop).map_err(ObjectStoreError::from)
    }

    // For some bizzare reason, `async fn` doesn't work here, failing with the following error:
    //
    // > hidden type for `impl std::future::Future<Output = Result<(), ObjectStoreError>>`
    // > captures lifetime that does not appear in bounds
    pub(crate) fn remove_async(
        &self,
        bucket: &'static str,
        key: &str,
    ) -> impl Future<Output = Result<(), ObjectStoreError>> + '_ {
        let filename = Self::filename(bucket, key);
        vlog::trace!(
            "Removing data from GCS for key {filename} from bucket {}",
            self.bucket_prefix
        );

        let request = DeleteObjectRequest {
            bucket: self.bucket_prefix.clone(),
            object: filename,
            ..DeleteObjectRequest::default()
        };
        async move {
            retry(self.max_retries, || self.client.delete_object(&request))
                .await
                .map_err(ObjectStoreError::from)
        }
    }
}

impl From<HttpError> for ObjectStoreError {
    fn from(err: HttpError) -> Self {
        let is_not_found = match &err {
            HttpError::HttpClient(err) => err
                .status()
                .map_or(false, |status| matches!(status, StatusCode::NOT_FOUND)),
            HttpError::Response(response) => response.code == StatusCode::NOT_FOUND.as_u16(),
            HttpError::TokenSource(_) => false,
        };

        if is_not_found {
            ObjectStoreError::KeyNotFound(err.into())
        } else {
            ObjectStoreError::Other(err.into())
        }
    }
}

#[derive(Debug)]
pub(crate) struct GoogleCloudStorage {
    inner: AsyncGoogleCloudStorage,
    handle: Handle,
}

impl GoogleCloudStorage {
    pub fn new(
        credential_file_path: Option<String>,
        bucket_prefix: String,
        max_retries: u16,
    ) -> Self {
        let handle = Handle::try_current().unwrap_or_else(|_| {
            panic!(
                "No Tokio runtime detected. Make sure that `dyn ObjectStore` is created \
                 on a Tokio thread, either in a task run by Tokio, or in the blocking context \
                 run with `tokio::task::spawn_blocking()`."
            );
        });
        let inner = AsyncGoogleCloudStorage::new(credential_file_path, bucket_prefix, max_retries);
        Self {
            inner: Self::block_on(&handle, inner),
            handle,
        }
    }

    fn block_on<T: Send>(handle: &Handle, future: impl Future<Output = T> + Send) -> T {
        if handle.runtime_flavor() == RuntimeFlavor::CurrentThread {
            // We would like to just call `handle.block_on(future)`, but this panics
            // if called in an async context. As such, we have this ugly hack, spawning
            // a new thread just to block on a future.
            thread::scope(|scope| {
                scope.spawn(|| handle.block_on(future)).join().unwrap()
                // ^ `unwrap()` propagates panics to the calling thread, which is what we want
            })
        } else {
            // In multi-threaded runtimes, we have `block_in_place` to the rescue.
            tokio::task::block_in_place(|| handle.block_on(future))
        }
    }
}

impl ObjectStore for GoogleCloudStorage {
    fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        let task = self.inner.get_async(bucket.as_str(), key);
        Self::block_on(&self.handle, task)
    }

    fn put_raw(&self, bucket: Bucket, key: &str, value: Vec<u8>) -> Result<(), ObjectStoreError> {
        let task = self.inner.put_async(bucket.as_str(), key, value);
        Self::block_on(&self.handle, task)
    }

    fn remove_raw(&self, bucket: Bucket, key: &str) -> Result<(), ObjectStoreError> {
        let task = self.inner.remove_async(bucket.as_str(), key);
        Self::block_on(&self.handle, task)
    }
}

#[cfg(test)]
mod test {
    use std::sync::atomic::{AtomicU16, Ordering};

    use super::*;

    async fn test_blocking() {
        let handle = Handle::current();
        let result = GoogleCloudStorage::block_on(&handle, async { 42 });
        assert_eq!(result, 42);

        let result = tokio::task::spawn_blocking(move || {
            GoogleCloudStorage::block_on(&handle, async { 42 })
        });
        assert_eq!(result.await.unwrap(), 42);
    }

    #[tokio::test]
    async fn blocking_in_sync_and_async_context() {
        test_blocking().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn blocking_in_sync_and_async_context_in_multithreaded_rt() {
        test_blocking().await;
    }

    #[tokio::test]
    async fn test_retry_success_immediate() {
        let result = retry(2, || async { Ok::<_, ()>(42) }).await;
        assert_eq!(result, Ok(42));
    }

    #[tokio::test]
    async fn test_retry_failure_exhausted() {
        let result = retry(2, || async { Err::<i32, _>(()) }).await;
        assert_eq!(result, Err(()));
    }

    async fn retry_success_after_n_retries(n: u16) -> Result<u32, String> {
        let retries = AtomicU16::new(0);
        let result = retry(n, || async {
            let retries = retries.fetch_add(1, Ordering::Relaxed);
            if retries + 1 == n {
                Ok(42)
            } else {
                Err(())
            }
        })
        .await;

        result.map_err(|_| "Retry failed".to_string())
    }

    #[tokio::test]
    async fn test_retry_success_after_retry() {
        let result = retry(2, || retry_success_after_n_retries(2)).await;
        assert_eq!(result, Ok(42));
    }
}
