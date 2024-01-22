use std::time::Duration;

use anyhow::anyhow;
use async_trait::async_trait;
use http::StatusCode;
use reqwest::Client;

use crate::{raw::BoxedError, Bucket, ObjectStore, ObjectStoreError};

#[derive(Debug)]
pub struct PublicReadOnlyGoogleCloudStorage {
    gcs_base_url: String,
    bucket_prefix: String,
    max_retries: u16,
    client: Client,
    backoff_seconds: u64,
}

impl PublicReadOnlyGoogleCloudStorage {
    pub fn new(
        gcs_base_url: String,
        bucket_prefix: String,
        max_retries: u16,
        timeout: u64,
        backoff_seconds: u64,
    ) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout))
            .build()
            .expect("Unable to build PublicReadOnlyGoogleCloudStorage http client from config");
        Self {
            gcs_base_url,
            bucket_prefix,
            max_retries,
            client,
            backoff_seconds,
        }
    }
}

#[async_trait]
impl ObjectStore for PublicReadOnlyGoogleCloudStorage {
    async fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        let mut last_error: Option<anyhow::Error> = None;
        let retry_count = self.max_retries;
        for retry_number in 0..retry_count {
            let url = format!("{}/{key}", self.storage_prefix_raw(bucket));
            let response = self.client.get(&url).send().await;

            match response {
                Ok(response) => {
                    let response_code = response.status();
                    match response_code {
                        StatusCode::NOT_FOUND => {
                            let error_message = format!(
                                "missing key: {key} in bucket {bucket} (got 404 from {url})"
                            );
                            return Err(ObjectStoreError::KeyNotFound(error_message.into()));
                        }
                        StatusCode::OK => {}
                        _ => {
                            let error_message = format!("unexpected error when fetching {key} from bucket {bucket}, received code {response_code}, url was {url}");
                            last_error = Some(anyhow!(error_message));
                            continue;
                        }
                    }
                    let bytes = response.bytes().await.map(|bytes| bytes.into());
                    match bytes {
                        Ok(bytes) => return Ok(bytes),
                        Err(error) => {
                            last_error = Some(error.into());
                            continue;
                        }
                    }
                }
                Err(error) => {
                    last_error = Some(error.into());
                }
            }

            tracing::warn!(
                "Failed to download {url} (attempt {}/{retry_count}). Backing off for {} seconds",
                retry_number + 1,
                self.backoff_seconds,
            );
            tokio::time::sleep(Duration::from_secs(self.backoff_seconds)).await;
        }
        Err(ObjectStoreError::Other(BoxedError::from(
            last_error.unwrap(),
        )))
    }

    async fn put_raw(
        &self,
        _bucket: Bucket,
        _key: &str,
        _value: Vec<u8>,
    ) -> Result<(), ObjectStoreError> {
        unimplemented!("This store is read-only!")
    }

    async fn remove_raw(&self, _bucket: Bucket, _key: &str) -> Result<(), ObjectStoreError> {
        unimplemented!("This store is read-only!")
    }

    fn storage_prefix_raw(&self, bucket: Bucket) -> String {
        format!(
            "{}/{}/{}",
            self.gcs_base_url,
            self.bucket_prefix.clone(),
            bucket.as_str()
        )
    }
}
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{
        gcs_public::PublicReadOnlyGoogleCloudStorage, Bucket, ObjectStore, ObjectStoreError,
    };

    #[tokio::test]
    async fn start_server() {
        let mut server = mockito::Server::new();

        // Use one of these addresses to configure your client
        let host = server.host_with_port();
        let url = server.url();

        let some_bytes = [1, 5, 124, 51];
        server
            .mock("GET", "/test-url/storage_logs_snapshots/some_key1")
            .with_status(200)
            .with_body(some_bytes)
            .create();

        server
            .mock("GET", "/test-url/storage_logs_snapshots/some_key2")
            .with_status(404)
            .create();

        server
            .mock("GET", "/test-url/storage_logs_snapshots/some_key3")
            .with_status(500)
            .create();

        let storage = PublicReadOnlyGoogleCloudStorage::new(url, "test-url".to_string(), 5, 1, 5);

        let bytes = storage
            .get_raw(Bucket::StorageSnapshot, "some_key1")
            .await
            .unwrap();
        assert_eq!(bytes, some_bytes);

        let not_found = storage
            .get_raw(Bucket::StorageSnapshot, "some_key2")
            .await
            .unwrap_err();
        assert!(matches!(not_found, ObjectStoreError::KeyNotFound { .. }));

        let server_error = storage
            .get_raw(Bucket::StorageSnapshot, "some_key3")
            .await
            .unwrap_err();
        assert!(matches!(server_error, ObjectStoreError::Other { .. }));
    }
}
