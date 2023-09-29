//! GCS-based [`ObjectStore`] implementation.

use async_trait::async_trait;
use google_cloud_auth::{credentials::CredentialsFile, error::Error};
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

use std::{fmt, future::Future, time::Duration};

use crate::{
    metrics::GCS_METRICS,
    raw::{Bucket, ObjectStore, ObjectStoreError},
};

async fn retry<T, E, Fut, F>(max_retries: u16, mut f: F) -> Result<T, E>
where
    E: fmt::Display,
    Fut: Future<Output = Result<T, E>>,
    F: FnMut() -> Fut,
{
    let mut retries = 1;
    let mut backoff = 1;
    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(err) => {
                tracing::warn!(%err, "Failed GCS request {retries}/{max_retries}, retrying.");
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

pub struct GoogleCloudStorage {
    bucket_prefix: String,
    max_retries: u16,
    client: Client,
}

impl fmt::Debug for GoogleCloudStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoogleCloudStorage")
            .field("bucket_prefix", &self.bucket_prefix)
            .field("max_retries", &self.max_retries)
            .finish_non_exhaustive()
    }
}

impl GoogleCloudStorage {
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

    // For some bizzare reason, `async fn` doesn't work here, failing with the following error:
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

#[async_trait]
impl ObjectStore for GoogleCloudStorage {
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
        let blob = retry(self.max_retries, || {
            self.client.download_object(&request, &range)
        })
        .await;

        let elapsed = fetch_latency.observe();
        tracing::trace!(
            "Fetched data from GCS for key {key} from bucket {bucket} and it took: {elapsed:?}"
        );
        blob.map_err(ObjectStoreError::from)
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
        let object = retry(self.max_retries, || {
            self.client
                .upload_object(&request, value.clone(), &upload_type)
        })
        .await;

        let elapsed = store_latency.observe();
        tracing::trace!(
            "Stored data to GCS for key {key} from bucket {bucket} and it took: {elapsed:?}"
        );
        object.map(drop).map_err(ObjectStoreError::from)
    }

    async fn remove_raw(&self, bucket: Bucket, key: &str) -> Result<(), ObjectStoreError> {
        self.remove_inner(bucket.as_str(), key).await
    }
}

#[cfg(test)]
mod test {
    use std::sync::atomic::{AtomicU16, Ordering};

    use super::*;

    #[tokio::test]
    async fn test_retry_success_immediate() {
        let result = retry(2, || async { Ok::<_, &'static str>(42) }).await;
        assert_eq!(result, Ok(42));
    }

    #[tokio::test]
    async fn test_retry_failure_exhausted() {
        let result = retry(2, || async { Err::<i32, _>("oops") }).await;
        assert_eq!(result, Err("oops"));
    }

    async fn retry_success_after_n_retries(n: u16) -> Result<u32, String> {
        let retries = AtomicU16::new(0);
        let result = retry(n, || async {
            let retries = retries.fetch_add(1, Ordering::Relaxed);
            if retries + 1 == n {
                Ok(42)
            } else {
                Err("oops")
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
