//! Caching object store.

use async_trait::async_trait;

use crate::{file::FileBackedObjectStore, raw::ObjectStore, Bucket, ObjectStoreError};

#[derive(Debug)]
pub(crate) struct CachingObjectStore<S> {
    inner: S,
    cache_store: FileBackedObjectStore,
}

impl<S: ObjectStore> CachingObjectStore<S> {
    pub async fn new(inner: S, cache_path: String) -> Result<Self, ObjectStoreError> {
        tracing::info!("Initializing caching for store {inner:?} at `{cache_path}`");
        let cache_store = FileBackedObjectStore::new(cache_path).await?;
        Ok(Self { inner, cache_store })
    }
}

#[async_trait]
impl<S: ObjectStore> ObjectStore for CachingObjectStore<S> {
    #[tracing::instrument(skip(self))]
    async fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        match self.cache_store.get_raw(bucket, key).await {
            Ok(object) => {
                tracing::trace!("obtained object from cache");
                return Ok(object);
            }
            Err(err) => {
                if !matches!(err, ObjectStoreError::KeyNotFound(_)) {
                    tracing::warn!(
                        "unexpected error calling cache store: {:#}",
                        anyhow::Error::from(err)
                    );
                }
                let object = self.inner.get_raw(bucket, key).await?;
                tracing::trace!("obtained object from underlying store");
                if let Err(err) = self.cache_store.put_raw(bucket, key, object.clone()).await {
                    tracing::warn!("failed caching object: {:#}", anyhow::Error::from(err));
                } else {
                    tracing::trace!("cached object");
                }
                Ok(object)
            }
        }
    }

    #[tracing::instrument(skip(self, value), fields(value.len = value.len()))]
    async fn put_raw(
        &self,
        bucket: Bucket,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), ObjectStoreError> {
        self.inner.put_raw(bucket, key, value.clone()).await?;
        // Only put the value into cache once it has been put in the underlying store
        if let Err(err) = self.cache_store.put_raw(bucket, key, value).await {
            tracing::warn!("failed caching object: {:#}", anyhow::Error::from(err));
        } else {
            tracing::trace!("cached object");
        }
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    async fn remove_raw(&self, bucket: Bucket, key: &str) -> Result<(), ObjectStoreError> {
        self.inner.remove_raw(bucket, key).await?;
        // Only remove the value from cache once it has been removed in the underlying store
        if let Err(err) = self.cache_store.remove_raw(bucket, key).await {
            tracing::warn!(
                "failed removing object from cache: {:#}",
                anyhow::Error::from(err)
            );
        } else {
            tracing::trace!("removed object from cache");
        }
        Ok(())
    }

    fn storage_prefix_raw(&self, bucket: Bucket) -> String {
        self.inner.storage_prefix_raw(bucket)
    }
}
