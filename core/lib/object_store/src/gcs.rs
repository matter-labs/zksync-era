//! GCS-based [`ObjectStore`] implementation.

use std::{error::Error as StdError, fmt, io, path::PathBuf};

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
use tokio::sync::{AcquireError, Semaphore};

use crate::raw::{Bucket, ObjectStore, ObjectStoreError};

/// Default maximum number of concurrent requests to GCS.
/// Consider this a throttle to prevent overwhelming GCS or network card.
/// The number has been picked after testing GCS in multiple conditions.
/// Do NOT change it without proper, thorough testing.
const DEFAULT_MAX_CONCURRENT_REQUESTS: usize = 500;

/// [`ObjectStore`] implementation based on GCS.
pub struct GoogleCloudStore {
    bucket_prefix: String,
    client: Client,
    // used to limit the number of concurrent requests to GCS.
    semaphore: Semaphore,
}

impl fmt::Debug for GoogleCloudStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoogleCloudStore")
            .field("bucket_prefix", &self.bucket_prefix)
            // Skip `client` as its representation may contain sensitive info
            .finish_non_exhaustive()
    }
}

/// Authentication mode for [`GoogleCloudStore`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum GoogleCloudStoreAuthMode {
    /// Authentication via a credentials file at the specified path.
    AuthenticatedWithCredentialFile(PathBuf),
    /// Ambient authentication (works if the binary runs on Google Cloud).
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
    ) -> Result<Self, ObjectStoreError> {
        let client_config = Self::get_client_config(auth_mode.clone()).await?;
        let semaphore = Semaphore::new(DEFAULT_MAX_CONCURRENT_REQUESTS);
        Ok(Self {
            client: Client::new(client_config),
            bucket_prefix,
            semaphore,
        })
    }

    /// Modifies the number of concurrent requests to GCS.
    /// NOTE: Big numbers can saturate the network and/or cause the GCS to be unavailable.
    #[must_use]
    pub fn with_request_limit(mut self, request_limit: usize) -> Self {
        self.semaphore = Semaphore::new(request_limit);
        self
    }

    async fn get_client_config(
        auth_mode: GoogleCloudStoreAuthMode,
    ) -> Result<ClientConfig, AuthError> {
        match auth_mode {
            GoogleCloudStoreAuthMode::AuthenticatedWithCredentialFile(path) => {
                // The `google_cloud_auth` API requests a string here (an owned one at that!), but converts it to a `Path` internally. Welp.
                let path = path.into_os_string().into_string().expect("non-UTF8 path");
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
}

impl From<AuthError> for ObjectStoreError {
    fn from(err: AuthError) -> Self {
        let is_retriable = matches!(
            &err,
            AuthError::HttpError(err) if is_retriable_http_error(err)
        );
        Self::Initialization {
            source: err.into(),
            is_retriable,
        }
    }
}

fn is_retriable_http_error(err: &reqwest::Error) -> bool {
    err.is_timeout()
        || err.is_connect()
        // Not all request errors are logically transient, but a significant part of them are (e.g.,
        // `hyper` protocol-level errors), and it's safer to consider an error retriable.
        || err.is_request()
        || has_transient_io_source(err)
        || err.status() == Some(StatusCode::BAD_GATEWAY)
        || err.status() == Some(StatusCode::SERVICE_UNAVAILABLE)
}

fn get_source<'a, T: StdError + 'static>(mut err: &'a (dyn StdError + 'static)) -> Option<&'a T> {
    loop {
        if let Some(err) = err.downcast_ref::<T>() {
            return Some(err);
        }
        err = err.source()?;
    }
}

fn has_transient_io_source(err: &(dyn StdError + 'static)) -> bool {
    // We treat any I/O errors as retriable. This isn't always true, but frequently occurring I/O errors
    // (e.g., "connection reset by peer") *are* transient, and treating an error as retriable is a safer option,
    // even if it can lead to unnecessary retries.
    get_source::<io::Error>(err).is_some()
}

/// This is necessary due to the fact that Acquiring a semaphore can fail if the semaphore is closed.
/// Whilst current code does not expect such scenario, nor "should it be possible", it is better to have a graceful catch case instead of unwrapping.
impl From<AcquireError> for ObjectStoreError {
    fn from(err: AcquireError) -> Self {
        ObjectStoreError::Other {
            source: err.into(),
            is_retriable: false,
        }
    }
}

impl From<HttpError> for ObjectStoreError {
    fn from(err: HttpError) -> Self {
        let is_not_found = match &err {
            HttpError::HttpClient(err) => err
                .status()
                .is_some_and(|status| matches!(status, StatusCode::NOT_FOUND)),
            HttpError::Response(response) => response.code == StatusCode::NOT_FOUND.as_u16(),
            _ => false,
        };

        if is_not_found {
            ObjectStoreError::KeyNotFound(err.into())
        } else {
            let is_retriable = match &err {
                HttpError::HttpClient(err) => is_retriable_http_error(err),
                HttpError::TokenSource(err) => {
                    // Token sources are mostly based on the `reqwest` HTTP client, so retriable error detection
                    // can reuse the same logic.
                    let err = err.as_ref();
                    has_transient_io_source(err)
                        || get_source::<reqwest::Error>(err).is_some_and(is_retriable_http_error)
                }
                HttpError::Response(err) => err.is_retriable(),
                _ => false,
            };
            ObjectStoreError::Other {
                is_retriable,
                source: err.into(),
            }
        }
    }
}

#[async_trait]
impl ObjectStore for GoogleCloudStore {
    async fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        let _permit = self.semaphore.acquire().await?;
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
        self.client
            .download_object(&request, &Range::default())
            .await
            .map_err(Into::into)
    }

    async fn put_raw(
        &self,
        bucket: Bucket,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), ObjectStoreError> {
        let _permit = self.semaphore.acquire().await?;
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
        self.client
            .upload_object(&request, value.clone(), &upload_type)
            .await?;
        Ok(())
    }

    async fn remove_raw(&self, bucket: Bucket, key: &str) -> Result<(), ObjectStoreError> {
        let _permit = self.semaphore.acquire().await?;
        let filename = Self::filename(bucket.as_str(), key);
        tracing::trace!(
            "Removing data from GCS for key {filename} from bucket {}",
            self.bucket_prefix
        );

        let request = DeleteObjectRequest {
            bucket: self.bucket_prefix.clone(),
            object: filename,
            ..DeleteObjectRequest::default()
        };
        self.client.delete_object(&request).await?;
        Ok(())
    }

    fn storage_prefix_raw(&self, bucket: Bucket) -> String {
        format!(
            "https://storage.googleapis.com/{}/{}",
            self.bucket_prefix.clone(),
            bucket.as_str()
        )
    }
}
