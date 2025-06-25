//! S3-based [`ObjectStore`] implementation.

use std::{fmt, path::PathBuf};

use anyhow::Context;
use async_trait::async_trait;
use aws_config::{meta::region::RegionProviderChain, BehaviorVersion, ConfigLoader, Region};
use aws_runtime::env_config::file::{EnvConfigFileKind, EnvConfigFiles};
use aws_sdk_s3::{error::SdkError, primitives::ByteStreamError, Client};
use http::StatusCode;

use crate::raw::{Bucket, ObjectStore, ObjectStoreError};

/// [`ObjectStore`] implementation based on AWS S3.
pub struct S3Store {
    endpoint: String,
    bucket_prefix: String,
    client: Client,
}

impl fmt::Debug for S3Store {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("S3Store")
            .field("bucket_prefix", &self.bucket_prefix)
            .field("endpoint", &self.endpoint)
            // Skip `client` as its representation may contain sensitive info
            .finish_non_exhaustive()
    }
}

/// Authentication mode for [`S3Store`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum S3StoreAuthMode {
    /// Authentication via a credentials file at the specified path.
    AuthenticatedWithCredentialFile(PathBuf),
    /// Anonymous access (only works for public buckets for read operations).
    Anonymous,
}

impl S3Store {
    /// Creates a new S3 store.
    pub async fn new(
        auth_mode: S3StoreAuthMode,
        bucket_prefix: String,
        endpoint: Option<String>,
        region: Option<String>,
    ) -> Result<Self, ObjectStoreError> {
        let region_provider = RegionProviderChain::first_try(region.map(Region::new))
            .or_default_provider()
            .or_else(Region::new("auto"));
        let mut sdk_config = Self::get_client_config(auth_mode).region(region_provider);
        if let Some(endpoint) = endpoint.clone() {
            tracing::info!(%endpoint, "using S3 endpoint defined in storage config");
            sdk_config = sdk_config.endpoint_url(endpoint);
        }
        let sdk_config = sdk_config.load().await;
        let client = Client::new(&sdk_config);

        Ok(Self {
            endpoint: endpoint.unwrap_or_default(),
            bucket_prefix,
            client,
        })
    }

    fn get_client_config(auth_mode: S3StoreAuthMode) -> ConfigLoader {
        match auth_mode {
            S3StoreAuthMode::AuthenticatedWithCredentialFile(path) => {
                let profile_files = EnvConfigFiles::builder()
                    .with_file(EnvConfigFileKind::Credentials, path)
                    .build();
                aws_config::defaults(BehaviorVersion::latest()).profile_files(profile_files)
            }
            S3StoreAuthMode::Anonymous => {
                aws_config::defaults(BehaviorVersion::latest()).no_credentials()
            }
        }
    }

    fn filename(bucket: &str, filename: &str) -> String {
        format!("{bucket}/{filename}")
    }
}

impl From<ByteStreamError> for ObjectStoreError {
    fn from(err: ByteStreamError) -> Self {
        ObjectStoreError::Other {
            source: err.into(),
            is_retriable: true,
        }
    }
}

impl From<anyhow::Error> for ObjectStoreError {
    fn from(err: anyhow::Error) -> Self {
        ObjectStoreError::Initialization {
            source: err.into(),
            is_retriable: false,
        }
    }
}

impl<T> From<SdkError<T>> for ObjectStoreError
where
    T: std::error::Error + Send + Sync + 'static,
{
    fn from(err: SdkError<T>) -> Self {
        match &err {
            SdkError::ConstructionFailure(_) => ObjectStoreError::Initialization {
                source: err.into(),
                is_retriable: false,
            },
            SdkError::DispatchFailure(_) | SdkError::TimeoutError(_) => ObjectStoreError::Other {
                source: err.into(),
                is_retriable: true,
            },
            SdkError::ResponseError(http_response) => {
                let code = http_response.raw().status().as_u16();
                if code == StatusCode::NOT_FOUND.as_u16() {
                    ObjectStoreError::KeyNotFound(err.into())
                } else if code == StatusCode::FORBIDDEN.as_u16() {
                    ObjectStoreError::Initialization {
                        source: err.into(),
                        is_retriable: false,
                    }
                } else {
                    ObjectStoreError::Other {
                        source: err.into(),
                        is_retriable: true,
                    }
                }
            }
            SdkError::ServiceError(http_response) => {
                let code = http_response.raw().status().as_u16();
                if code == StatusCode::NOT_FOUND.as_u16() {
                    ObjectStoreError::KeyNotFound(err.into())
                } else if code == StatusCode::FORBIDDEN.as_u16() {
                    ObjectStoreError::Initialization {
                        source: err.into(),
                        is_retriable: false,
                    }
                } else {
                    ObjectStoreError::Other {
                        source: err.into(),
                        is_retriable: true,
                    }
                }
            }
            _ => ObjectStoreError::Other {
                source: err.into(),
                is_retriable: false,
            },
        }
    }
}

#[async_trait]
impl ObjectStore for S3Store {
    async fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        let filename = Self::filename(bucket.as_str(), key);
        tracing::trace!(
            "Fetching data from S3 for key {filename} from bucket {}",
            self.bucket_prefix
        );

        let get_object_output = self
            .client
            .get_object()
            .bucket(self.bucket_prefix.clone())
            .key(filename)
            .send()
            .await?;
        Ok(get_object_output.body.collect().await?.to_vec())
    }

    async fn put_raw(
        &self,
        bucket: Bucket,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), ObjectStoreError> {
        let filename = Self::filename(bucket.as_str(), key);
        tracing::trace!(
            "Storing data to S3 for key {filename} from bucket {}",
            self.bucket_prefix
        );

        let length = i64::try_from(value.len()).context("Object is way too big")?;
        self.client
            .put_object()
            .bucket(self.bucket_prefix.clone())
            .key(filename)
            .body(value.into())
            .content_length(length)
            .send()
            .await?;
        Ok(())
    }

    async fn remove_raw(&self, bucket: Bucket, key: &str) -> Result<(), ObjectStoreError> {
        let filename = Self::filename(bucket.as_str(), key);
        tracing::trace!(
            "Removing data from S3 for key {filename} from bucket {}",
            self.bucket_prefix
        );

        self.client
            .delete_object()
            .bucket(self.bucket_prefix.clone())
            .key(filename)
            .send()
            .await?;
        Ok(())
    }

    fn storage_prefix_raw(&self, bucket: Bucket) -> String {
        format!(
            "{}/{}/{}",
            self.endpoint.clone(),
            self.bucket_prefix.clone(),
            bucket.as_str()
        )
    }
}
