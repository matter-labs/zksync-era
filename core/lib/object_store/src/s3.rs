use crate::raw::PreparedLink;
use crate::{Bucket, ObjectStore, ObjectStoreError};
use async_trait::async_trait;
use http::Method;
use object_store::signer::Signer;
use object_store::ObjectStore as _;
use object_store::{
    aws::{AmazonS3, AmazonS3Builder},
    path::Path,
};

#[derive(Debug)]
pub struct S3Store {
    s3: AmazonS3,
    prepared_links_lifetime: u64,
}

impl S3Store {
    /// Initialize and S3-backed [`ObjectStore`] from the provided credentials.
    pub async fn from_keys(
        prepared_links_lifetime: u64,
        endpoint: Option<String>,
        region: &str,
        bucket: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Result<Self, ObjectStoreError> {
        let mut s3_builder = AmazonS3Builder::new()
            .with_region(region)
            .with_bucket_name(bucket)
            .with_access_key_id(access_key)
            .with_secret_access_key(secret_key);
        if let Some(endpoint) = endpoint {
            s3_builder = s3_builder.with_endpoint(endpoint);
        }

        Ok(Self {
            s3: s3_builder.build().map_err(|e| ObjectStoreError::from(e))?,
            prepared_links_lifetime,
        })
    }

    /// Initialize an S3-backed [`ObjectStore`] from the credentials stored in
    /// `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`.
    pub async fn from_env(
        prepared_links_lifetime: u64,
        endpoint: Option<String>,
        region: &str,
        bucket: &str,
    ) -> Result<Self, ObjectStoreError> {
        let mut s3_builder = AmazonS3Builder::from_env()
            .with_region(region)
            .with_bucket_name(bucket);
        if let Some(endpoint) = endpoint {
            s3_builder = s3_builder.with_endpoint(endpoint);
        }

        Ok(Self {
            s3: s3_builder.build().map_err(|e| ObjectStoreError::from(e))?,
            prepared_links_lifetime,
        })
    }
}

impl From<object_store::Error> for ObjectStoreError {
    fn from(e: object_store::Error) -> Self {
        match e {
            object_store::Error::Generic { source, .. } => ObjectStoreError::Other {
                source,
                is_retriable: false,
            },
            object_store::Error::NotFound { source, .. } => ObjectStoreError::KeyNotFound(source),
            object_store::Error::InvalidPath { source } => ObjectStoreError::Other {
                source: Box::new(source),
                is_retriable: false,
            },
            object_store::Error::JoinError { source } => ObjectStoreError::Other {
                source: Box::new(source),
                is_retriable: false,
            },
            object_store::Error::NotSupported { source }
            | object_store::Error::AlreadyExists { source, .. }
            | object_store::Error::Precondition { source, .. }
            | object_store::Error::Unauthenticated { source, .. }
            | object_store::Error::NotModified { source, .. } => ObjectStoreError::Other {
                source,
                is_retriable: false,
            },
            object_store::Error::NotImplemented => ObjectStoreError::Other {
                source: Box::from("unimplemented"),
                is_retriable: false,
            },
            object_store::Error::PermissionDenied { path, .. } => ObjectStoreError::Other {
                source: Box::from(format!("access denied to `{path}`")),
                is_retriable: false,
            },
            object_store::Error::UnknownConfigurationKey { key, .. } => {
                ObjectStoreError::Initialization {
                    source: Box::from(format!("key `{key}` unknown")),
                    is_retriable: false,
                }
            }
            _ => todo!(),
        }
    }
}

fn qualifed_key(bucket: &Bucket, key: &str) -> String {
    format!("{bucket}/{key}")
}

#[async_trait]
impl ObjectStore for S3Store {
    async fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        tracing::trace!("Fetching data to S3 for key {key} from bucket {bucket}");
        Ok(self
            .s3
            .get(&Path::from(qualifed_key(&bucket, key)))
            .await
            .map_err(ObjectStoreError::from)?
            .bytes()
            .await
            .map_err(ObjectStoreError::from)?
            .to_vec())
    }

    async fn put_raw(
        &self,
        bucket: Bucket,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), ObjectStoreError> {
        tracing::trace!("Storing data to S3 for key {key} from bucket {bucket}");
        self.s3
            .put(&Path::from(qualifed_key(&bucket, key)), value.into())
            .await
            .map(|_| ())
            .map_err(ObjectStoreError::from)
    }

    async fn remove_raw(&self, bucket: Bucket, key: &str) -> Result<(), ObjectStoreError> {
        self.s3
            .delete(&Path::from(qualifed_key(&bucket, key)))
            .await
            .map(|_| ())
            .map_err(ObjectStoreError::from)
    }

    fn storage_prefix_raw(&self, _bucket: Bucket) -> String {
        unimplemented!()
    }

    async fn prepare_download(
        &self,
        bucket: Bucket,
        key: &str,
    ) -> Result<PreparedLink, ObjectStoreError> {
        let url = self
            .s3
            .signed_url(
                Method::GET,
                &Path::from(qualifed_key(&bucket, key)),
                std::time::Duration::from_secs(60 * self.prepared_links_lifetime),
            )
            .await
            .map_err(|e| ObjectStoreError::Other {
                source: Box::new(e),
                is_retriable: false,
            })?
            .to_string();
        Ok(PreparedLink::Url(url))
    }

    async fn prepare_upload(
        &self,
        bucket: Bucket,
        key: &str,
    ) -> Result<PreparedLink, ObjectStoreError> {
        let url = self
            .s3
            .signed_url(
                Method::PUT,
                &Path::from(qualifed_key(&bucket, key)),
                std::time::Duration::from_secs(60 * self.prepared_links_lifetime),
            )
            .await
            .map_err(|e| ObjectStoreError::Other {
                source: Box::new(e),
                is_retriable: false,
            })?
            .to_string();
        Ok(PreparedLink::Url(url))
    }
}
