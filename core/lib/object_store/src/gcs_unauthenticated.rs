use std::time::Duration;

use async_trait::async_trait;
use http::StatusCode;
use reqwest::Error;

use crate::{raw::BoxedError, Bucket, ObjectStore, ObjectStoreError};

#[derive(Debug)]
pub struct UnauthenticatedGoogleCloudStorage {
    bucket_prefix: String,
    max_retries: u16,
}

#[async_trait]
impl ObjectStore for UnauthenticatedGoogleCloudStorage {
    async fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
        let client = reqwest::Client::builder()
            .timeout(DOWNLOAD_TIMEOUT)
            .build()
            .map_err(|error| ObjectStoreError::Other(error.into()))?;

        let mut last_error: Option<Error> = None;
        let retry_count = self.max_retries;
        for retry_number in 0..retry_count {
            let url = format!("{}/{key}", self.storage_prefix_raw(bucket));
            let response = client.get(url.to_string()).send().await;

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
                            let error_message = format!("unexpected error when fetching {key} from bucket {bucket}, received code {response_code}");
                            return Err(ObjectStoreError::Other(error_message.into()));
                        }
                    }
                    let bytes = response.bytes().await.map(|bytes| bytes.to_vec());
                    match bytes {
                        Ok(bytes) => return Ok(bytes),
                        Err(error) => {
                            last_error = Some(error);
                        }
                    }
                }
                Err(error) => {
                    last_error = Some(error);
                }
            }

            tracing::warn!(
                "Failed to download {url} (attempt {}/{retry_count}). Backing off for 5 second",
                retry_number + 1
            );
            tokio::time::sleep(Duration::from_secs(5)).await;
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
            "https://storage.googleapis.com/{}/{}",
            self.bucket_prefix.clone(),
            bucket.as_str()
        )
    }
}
