//! S3-based [`ObjectStore`] implementation.
use async_trait::async_trait;
use aws_sdk_s3::{
    error::SdkError,
    operation::{
        delete_object::DeleteObjectError, get_object::GetObjectError, put_object::PutObjectError,
    },
    primitives::{ByteStream, ByteStreamError},
    Client,
};

use tokio::runtime::{Handle, RuntimeFlavor};

use std::{fmt, future::Future, thread, time::Instant};

use crate::raw::{Bucket, ObjectStore, ObjectStoreError};

pub struct AsyncS3Storage {
    bucket_url: String,
    client: Client,
}

impl fmt::Debug for AsyncS3Storage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("S3StorageAsync")
            .field("bucket_url", &self.bucket_url)
            .finish()
    }
}

impl AsyncS3Storage {
    pub async fn new(endpoint_url: String, bucket_url: String) -> Self {
        let shared_config = aws_config::load_from_env().await;
        let config = aws_sdk_s3::config::Builder::from(&shared_config)
            .endpoint_url(endpoint_url)
            .build();

        let client = Client::from_conf(config);
        Self { client, bucket_url }
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
            "Fetching data from S3 for key {filename} from bucket {}",
            self.bucket_url
        );

        let object = self
            .client
            .get_object()
            .bucket(self.bucket_url.clone())
            .key(filename.clone())
            .send()
            .await?;

        vlog::trace!(
            "Fetched data from S3 for key {filename} from bucket {} and it took: {:?}",
            self.bucket_url,
            started_at.elapsed()
        );

        metrics::histogram!(
            "server.object_store.fetching_time",
            started_at.elapsed(),
            "bucket" => bucket
        );

        object
            .body
            .collect()
            .await
            .map(|data| data.to_vec())
            .map_err(ObjectStoreError::from)
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
            "Storing data to s3 for key {filename} from bucket {}",
            self.bucket_url
        );

        let body = ByteStream::from(value);

        let result = self
            .client
            .put_object()
            .bucket(self.bucket_url.clone())
            .key(filename.clone())
            .body(body)
            .send()
            .await?;

        vlog::trace!("PutObjectOutput {:?}", result);

        vlog::trace!(
            "Stored data to S3 for key {filename} from bucket {} and it took: {:?}",
            self.bucket_url,
            started_at.elapsed()
        );
        metrics::histogram!(
            "server.object_store.storing_time",
            started_at.elapsed(),
            "bucket" => bucket
        );
        Ok(())
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
            "Removing data from S3 for key {filename} from bucket {}",
            self.bucket_url
        );

        let request = self
            .client
            .delete_object()
            .bucket(self.bucket_url.clone())
            .key(filename.clone());

        async move {
            match request.send().await {
                Ok(_s) => Ok(()),
                Err(error) => return Err(ObjectStoreError::Other(error.into())),
            }
        }
    }
}

impl From<SdkError<GetObjectError>> for ObjectStoreError {
    fn from(sdk_err: SdkError<GetObjectError>) -> Self {
        let err = sdk_err.into_service_error();
        match err {
            GetObjectError::InvalidObjectState(_) => ObjectStoreError::KeyNotFound(err.into()),
            GetObjectError::NoSuchKey(_) => ObjectStoreError::KeyNotFound(err.into()),
            GetObjectError::Unhandled(_) => ObjectStoreError::Other(err.into()),
            _ => todo!(),
        }
    }
}

impl From<SdkError<PutObjectError>> for ObjectStoreError {
    fn from(sdk_err: SdkError<PutObjectError>) -> Self {
        let err = sdk_err.into_service_error();
        match err {
            PutObjectError::Unhandled(_) => ObjectStoreError::Other(err.into()),
            _ => todo!(),
        }
    }
}

impl From<SdkError<DeleteObjectError>> for ObjectStoreError {
    fn from(sdk_err: SdkError<DeleteObjectError>) -> Self {
        let err = sdk_err.into_service_error();
        match err {
            DeleteObjectError::Unhandled(_) => ObjectStoreError::Other(err.into()),
            _ => todo!(),
        }
    }
}

impl From<ByteStreamError> for ObjectStoreError {
    fn from(byte_stream_err: ByteStreamError) -> Self {
        ObjectStoreError::Other(byte_stream_err.into())
    }
}

#[derive(Debug)]
pub(crate) struct S3Storage {
    inner: AsyncS3Storage,
    handle: Handle,
}

impl S3Storage {
    pub fn new(endpoint_url: String, bucket_url: String, _max_retries: u16) -> Self {
        let handle = Handle::try_current().unwrap_or_else(|_| {
            panic!(
                "No Tokio runtime detected. Make sure that `dyn ObjectStore` is created \
                 on a Tokio thread, either in a task run by Tokio, or in the blocking context \
                 run with `tokio::task::spawn_blocking()`."
            );
        });

        // TODO: S3 client has default 2 retries. we can pass max_retries to the s3 retry config

        let inner = AsyncS3Storage::new(endpoint_url, bucket_url);
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

#[async_trait]
impl ObjectStore for S3Storage {
    async fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        let task = self.inner.get_async(bucket.as_str(), key);
        Self::block_on(&self.handle, task)
    }

    async fn put_raw(&self, bucket: Bucket, key: &str, value: Vec<u8>) -> Result<(), ObjectStoreError> {
        let task = self.inner.put_async(bucket.as_str(), key, value);
        Self::block_on(&self.handle, task)
    }

    async fn remove_raw(&self, bucket: Bucket, key: &str) -> Result<(), ObjectStoreError> {
        let task = self.inner.remove_async(bucket.as_str(), key);
        Self::block_on(&self.handle, task)
    }
}

#[cfg(test)]
mod test {

    use super::*;

    async fn test_blocking() {
        let handle = Handle::current();
        let result = S3Storage::block_on(&handle, async { 42 });
        assert_eq!(result, 42);

        let result =
            tokio::task::spawn_blocking(move || S3Storage::block_on(&handle, async { 42 }));
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
}