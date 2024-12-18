//! Mirroring object store.

use std::path::PathBuf;

use async_trait::async_trait;

use crate::{file::FileBackedObjectStore, raw::ObjectStore, Bucket, ObjectStoreError};

#[derive(Debug)]
pub(crate) struct MirroringObjectStore<S> {
    inner: S,
    mirror_store: FileBackedObjectStore,
}

impl<S: ObjectStore> MirroringObjectStore<S> {
    pub async fn new(inner: S, mirror_path: PathBuf) -> Result<Self, ObjectStoreError> {
        tracing::info!(
            "Initializing mirroring for store {inner:?} at `{}`",
            mirror_path.display()
        );
        let mirror_store = FileBackedObjectStore::new(mirror_path).await?;
        Ok(Self {
            inner,
            mirror_store,
        })
    }
}

#[async_trait]
impl<S: ObjectStore> ObjectStore for MirroringObjectStore<S> {
    #[tracing::instrument(name = "MirroringObjectStore::get_raw", skip(self))]
    async fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        match self.mirror_store.get_raw(bucket, key).await {
            Ok(object) => {
                tracing::trace!("obtained object from mirror");
                return Ok(object);
            }
            Err(err) => {
                if !matches!(err, ObjectStoreError::KeyNotFound(_)) {
                    tracing::warn!(
                        "unexpected error calling local mirror store: {:#}",
                        anyhow::Error::from(err)
                    );
                }
                let object = self.inner.get_raw(bucket, key).await?;
                tracing::trace!("obtained object from underlying store");
                if let Err(err) = self.mirror_store.put_raw(bucket, key, object.clone()).await {
                    tracing::warn!("failed mirroring object: {:#}", anyhow::Error::from(err));
                } else {
                    tracing::trace!("mirrored object");
                }
                Ok(object)
            }
        }
    }

    #[tracing::instrument(
        name = "MirroringObjectStore::put_raw",
        skip(self, value),
        fields(value.len = value.len())
    )]
    async fn put_raw(
        &self,
        bucket: Bucket,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), ObjectStoreError> {
        self.inner.put_raw(bucket, key, value.clone()).await?;
        // Only put the value into the mirror once it has been put in the underlying store
        if let Err(err) = self.mirror_store.put_raw(bucket, key, value).await {
            tracing::warn!("failed mirroring object: {:#}", anyhow::Error::from(err));
        } else {
            tracing::trace!("mirrored object");
        }
        Ok(())
    }

    #[tracing::instrument(name = "MirroringObjectStore::remove_raw", skip(self))]
    async fn remove_raw(&self, bucket: Bucket, key: &str) -> Result<(), ObjectStoreError> {
        self.inner.remove_raw(bucket, key).await?;
        // Only remove the value from the mirror once it has been removed in the underlying store
        if let Err(err) = self.mirror_store.remove_raw(bucket, key).await {
            tracing::warn!(
                "failed removing object from mirror: {:#}",
                anyhow::Error::from(err)
            );
        } else {
            tracing::trace!("removed object from mirror");
        }
        Ok(())
    }

    fn storage_prefix_raw(&self, bucket: Bucket) -> String {
        self.inner.storage_prefix_raw(bucket)
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use tempfile::TempDir;

    use super::*;
    use crate::MockObjectStore;

    #[tokio::test]
    async fn mirroring_basics() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_owned();

        let mock_store = MockObjectStore::default();
        mock_store
            .put_raw(Bucket::StorageSnapshot, "test", vec![1, 2, 3])
            .await
            .unwrap();
        let mirroring_store = MirroringObjectStore::new(mock_store, path).await.unwrap();

        let object = mirroring_store
            .get_raw(Bucket::StorageSnapshot, "test")
            .await
            .unwrap();
        assert_eq!(object, [1, 2, 3]);
        // Check that the object got mirrored.
        let object_in_mirror = mirroring_store
            .mirror_store
            .get_raw(Bucket::StorageSnapshot, "test")
            .await
            .unwrap();
        assert_eq!(object_in_mirror, [1, 2, 3]);
        let object = mirroring_store
            .get_raw(Bucket::StorageSnapshot, "test")
            .await
            .unwrap();
        assert_eq!(object, [1, 2, 3]);

        let err = mirroring_store
            .get_raw(Bucket::StorageSnapshot, "missing")
            .await
            .unwrap_err();
        assert_matches!(err, ObjectStoreError::KeyNotFound(_));

        mirroring_store
            .put_raw(Bucket::StorageSnapshot, "other", vec![3, 2, 1])
            .await
            .unwrap();
        // Check that the object got mirrored.
        let object_in_mirror = mirroring_store
            .mirror_store
            .get_raw(Bucket::StorageSnapshot, "other")
            .await
            .unwrap();
        assert_eq!(object_in_mirror, [3, 2, 1]);
        let object = mirroring_store
            .get_raw(Bucket::StorageSnapshot, "other")
            .await
            .unwrap();
        assert_eq!(object, [3, 2, 1]);
    }
}
