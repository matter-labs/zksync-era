//! GCS-based [`ObjectStore`] implementation.

use std::{fmt, future::Future, time::Duration};

use async_trait::async_trait;
use google_cloud_auth::{credentials::CredentialsFile, error::Error as AuthError};
use google_cloud_storage::{
    client::{Client, ClientConfig},
    http::{
        objects::{
            delete::DeleteObjectRequest,
            download::Range,
            get::GetObjectRequest,
            upload::{Media, UploadObjectRequest, UploadType},
        },
        Error as HttpError,
    },
};
use http::StatusCode;
use rand::Rng;

use crate::{
    metrics::GCS_METRICS,
    raw::{Bucket, ObjectStore, ObjectStoreError},
};

async fn retry<T, Fut, F>(max_retries: u16, mut f: F) -> Result<T, ObjectStoreError>
where
    Fut: Future<Output = Result<T, ObjectStoreError>>,
    F: FnMut() -> Fut,
{
    let mut retries = 1;
    let mut backoff_secs = 1;
    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(err) if err.is_transient() => {
                if retries > max_retries {
                    tracing::warn!(%err, "Exhausted {max_retries} retries performing GCS request; returning last error");
                    return Err(err);
                }
                tracing::info!(%err, "Failed GCS request {retries}/{max_retries}, retrying.");
                retries += 1;
                // Randomize sleep duration to prevent stampeding the server if multiple requests are initiated at the same time.
                let sleep_duration = Duration::from_secs(backoff_secs)
                    .mul_f32(rand::thread_rng().gen_range(0.8..1.2));
                tokio::time::sleep(sleep_duration).await;
                backoff_secs *= 2;
            }
            Err(err) => {
                tracing::warn!(%err, "Failed GCS request with a fatal error");
                return Err(err);
            }
        }
    }
}

/// [`ObjectStore`] implementation based on GCS.
pub struct GoogleCloudStore {
    bucket_prefix: String,
    max_retries: u16,
    client: Client,
}

impl fmt::Debug for GoogleCloudStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoogleCloudStore")
            .field("bucket_prefix", &self.bucket_prefix)
            .field("max_retries", &self.max_retries)
            // Skip `client` as its representation may contain sensitive info
            .finish_non_exhaustive()
    }
}

/// Authentication mode for [`GoogleCloudStore`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum GoogleCloudStoreAuthMode {
    /// Authentication via a credentials file at the specified path.
    AuthenticatedWithCredentialFile(String),
    /// Ambient authentication (works if )
    Authenticated,
    /// Anonymous access (only works for public GCS buckets for read operations).
    Anonymous,
}

impl GoogleCloudStore {
    /// Creates a new cloud store.
    ///
    /// # Errors
    ///
    /// Propagates GCS initialization errors.
    pub async fn new(
        auth_mode: GoogleCloudStoreAuthMode,
        bucket_prefix: String,
        max_retries: u16,
    ) -> Result<Self, ObjectStoreError> {
        let client_config = retry(max_retries, || async {
            Self::get_client_config(auth_mode.clone())
                .await
                .map_err(Into::into)
        })
        .await?;

        Ok(Self {
            client: Client::new(client_config),
            bucket_prefix,
            max_retries,
        })
    }

    async fn get_client_config(
        auth_mode: GoogleCloudStoreAuthMode,
    ) -> Result<ClientConfig, AuthError> {
        match auth_mode {
            GoogleCloudStoreAuthMode::AuthenticatedWithCredentialFile(path) => {
                let cred_file = CredentialsFile::new_from_file(path).await?;
                ClientConfig::default().with_credentials(cred_file).await
            }
            GoogleCloudStoreAuthMode::Authenticated => ClientConfig::default().with_auth().await,
            GoogleCloudStoreAuthMode::Anonymous => Ok(ClientConfig::default().anonymous()),
        }
    }

    fn filename(bucket: &str, filename: &str) -> String {
        format!("{bucket}/{filename}")
    }

    // For some bizarre reason, `async fn` doesn't work here, failing with the following error:
    //
    // > hidden type for `impl std::future::Future<Output = Result<(), ObjectStoreError>>`
    // > captures lifetime that does not appear in bounds
    fn remove_inner(
        &self,
        bucket: &'static str,
        key: &str,
    ) -> impl Future<Output = Result<(), ObjectStoreError>> + '_ {
        let filename = Self::filename(bucket, key);
        tracing::trace!(
            "Removing data from GCS for key {filename} from bucket {}",
            self.bucket_prefix
        );

        let request = DeleteObjectRequest {
            bucket: self.bucket_prefix.clone(),
            object: filename,
            ..DeleteObjectRequest::default()
        };
        async move {
            retry(self.max_retries, || async {
                self.client
                    .delete_object(&request)
                    .await
                    .map_err(ObjectStoreError::from)
            })
            .await
        }
    }
}

impl From<AuthError> for ObjectStoreError {
    fn from(err: AuthError) -> Self {
        let is_transient =
            matches!(&err, AuthError::HttpError(err) if err.is_timeout() || err.is_connect());
        Self::Initialization {
            source: err.into(),
            is_transient,
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
            let is_transient =
                matches!(&err, HttpError::HttpClient(err) if err.is_timeout() || err.is_connect());
            ObjectStoreError::Other {
                is_transient,
                source: err.into(),
            }
        }
    }
}

#[async_trait]
impl ObjectStore for GoogleCloudStore {
    async fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        let fetch_latency = GCS_METRICS.start_fetch(bucket);
        let filename = Self::filename(bucket.as_str(), key);
        tracing::trace!(
            "Fetching data from GCS for key {filename} from bucket {}",
            self.bucket_prefix
        );

        let request = GetObjectRequest {
            bucket: self.bucket_prefix.clone(),
            object: filename,
            ..GetObjectRequest::default()
        };
        let range = Range::default();
        let blob = retry(self.max_retries, || async {
            self.client
                .download_object(&request, &range)
                .await
                .map_err(Into::into)
        })
        .await;

        let elapsed = fetch_latency.observe();
        tracing::trace!(
            "Fetched data from GCS for key {key} from bucket {bucket} and it took: {elapsed:?}"
        );
        blob
    }

    async fn put_raw(
        &self,
        bucket: Bucket,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), ObjectStoreError> {
        let store_latency = GCS_METRICS.start_store(bucket);
        let filename = Self::filename(bucket.as_str(), key);
        tracing::trace!(
            "Storing data to GCS for key {filename} from bucket {}",
            self.bucket_prefix
        );

        let upload_type = UploadType::Simple(Media::new(filename));
        let request = UploadObjectRequest {
            bucket: self.bucket_prefix.clone(),
            ..Default::default()
        };
        let object = retry(self.max_retries, || async {
            self.client
                .upload_object(&request, value.clone(), &upload_type)
                .await
                .map_err(Into::into)
        })
        .await;

        let elapsed = store_latency.observe();
        tracing::trace!(
            "Stored data to GCS for key {key} from bucket {bucket} and it took: {elapsed:?}"
        );
        object.map(drop)
    }

    async fn remove_raw(&self, bucket: Bucket, key: &str) -> Result<(), ObjectStoreError> {
        self.remove_inner(bucket.as_str(), key).await
    }

    fn storage_prefix_raw(&self, bucket: Bucket) -> String {
        format!(
            "https://storage.googleapis.com/{}/{}",
            self.bucket_prefix.clone(),
            bucket.as_str()
        )
    }
}

#[cfg(test)]
mod test {
    use std::sync::atomic::{AtomicU16, Ordering};

    use assert_matches::assert_matches;

    use super::*;

    fn transient_error() -> ObjectStoreError {
        ObjectStoreError::Other {
            is_transient: true,
            source: "oops".into(),
        }
    }

    #[tokio::test]
    async fn test_retry_success_immediate() {
        let result = retry(2, || async { Ok(42) }).await.unwrap();
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_retry_failure_exhausted() {
        let err = retry(2, || async { Err::<i32, _>(transient_error()) })
            .await
            .unwrap_err();
        assert_matches!(err, ObjectStoreError::Other { .. });
    }

    async fn retry_success_after_n_retries(n: u16) -> Result<u32, ObjectStoreError> {
        let retries = AtomicU16::new(0);
        retry(n, || async {
            let retries = retries.fetch_add(1, Ordering::Relaxed);
            if retries + 1 == n {
                Ok(42)
            } else {
                Err(transient_error())
            }
        })
        .await
    }

    #[tokio::test]
    async fn test_retry_success_after_retry() {
        let result = retry(2, || retry_success_after_n_retries(2)).await.unwrap();
        assert_eq!(result, 42);
    }
}
