use async_trait::async_trait;
use s3::creds::Credentials;
use s3::error::S3Error;
use s3::{bucket::Bucket as S3Bucket, Region};

use crate::{Bucket, ObjectStore, ObjectStoreError};

#[derive(Debug)]
pub struct S3Store {
    bucket: Box<S3Bucket>,
}

impl S3Store {
    /// Initialize and S3-backed [`ObjectStore`] from the provided credentials.
    pub async fn from_keys(
        bucket: String,
        region: String,
        access_key: &str,
        secret_key: &str,
    ) -> Result<Self, ObjectStoreError> {
        let creds = Credentials::new(Some(access_key), Some(secret_key), None, None, None)
            .map_err(|e| ObjectStoreError::Initialization {
                source: Box::new(e),
                is_retriable: false,
            })?;
        let region: Region = region
            .parse()
            .map_err(|e| ObjectStoreError::Initialization {
                source: Box::new(e),
                is_retriable: false,
            })?;
        let bucket =
            S3Bucket::new(bucket.as_str(), region.clone(), creds.clone()).map_err(|e| {
                ObjectStoreError::Other {
                    source: Box::new(e),
                    is_retriable: false,
                }
            })?;

        Ok(Self { bucket })
    }

    /// Initialize an S3-backed [`ObjectStore`] from the credentials stored in
    /// `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`.
    pub async fn from_env(bucket: String, region: String) -> Result<Self, ObjectStoreError> {
        let creds = Credentials::new(None, None, None, None, None).map_err(|e| {
            ObjectStoreError::Initialization {
                source: Box::new(e),
                is_retriable: false,
            }
        })?;
        let region: Region = region
            .parse()
            .map_err(|e| ObjectStoreError::Initialization {
                source: Box::new(e),
                is_retriable: false,
            })?;
        let bucket =
            S3Bucket::new(bucket.as_str(), region.clone(), creds.clone()).map_err(|e| {
                ObjectStoreError::Other {
                    source: Box::new(e),
                    is_retriable: false,
                }
            })?;

        Ok(Self { bucket })
    }
}

impl From<S3Error> for ObjectStoreError {
    fn from(e: S3Error) -> Self {
        match e {
            S3Error::Credentials(_) | S3Error::Region(_) => ObjectStoreError::Initialization {
                source: Box::new(e),
                is_retriable: false,
            },

            S3Error::Utf8(_)
            | S3Error::MaxExpiry(_)
            | S3Error::HttpFailWithBody(_, _)
            | S3Error::HttpFail
            | S3Error::HmacInvalidLength(_)
            | S3Error::UrlParse(_)
            | S3Error::NativeTls(_)
            | S3Error::HeaderToStr(_)
            | S3Error::FromUtf8(_)
            | S3Error::SerdeXml(_)
            | S3Error::InvalidHeaderValue(_)
            | S3Error::InvalidHeaderName(_)
            | S3Error::WLCredentials
            | S3Error::RLCredentials
            | S3Error::TimeFormatError(_)
            | S3Error::FmtError(_)
            | S3Error::PostPolicyError(_)
            | S3Error::CredentialsReadLock
            | S3Error::CredentialsWriteLock => ObjectStoreError::Other {
                source: Box::new(e),
                is_retriable: false,
            },
            S3Error::SerdeError(serde_err) => ObjectStoreError::Serialization(Box::new(serde_err)),
            S3Error::Http(_http_err) => todo!(),
            S3Error::Io(_) | S3Error::Hyper(_) => todo!(),
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
        self.bucket
            .get_object(qualifed_key(&bucket, key))
            .await
            .map(|r| r.to_vec())
            .map_err(ObjectStoreError::from)
    }

    async fn put_raw(
        &self,
        bucket: Bucket,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), ObjectStoreError> {
        tracing::trace!("Storing data to S3 for key {key} from bucket {bucket}");
        self.bucket
            .put_object(qualifed_key(&bucket, key), &value)
            .await
            .map(|_| ())
            .map_err(ObjectStoreError::from)
    }

    async fn remove_raw(&self, bucket: Bucket, key: &str) -> Result<(), ObjectStoreError> {
        self.bucket
            .delete_object(qualifed_key(&bucket, key))
            .await
            .map(|_| ())
            .map_err(ObjectStoreError::from)
    }

    fn storage_prefix_raw(&self, _bucket: Bucket) -> String {
        self.bucket.url()
    }
}
