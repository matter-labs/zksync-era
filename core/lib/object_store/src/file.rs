use std::{fmt::Debug, path::PathBuf};

use async_trait::async_trait;
use tokio::{fs, io};

use crate::raw::{Bucket, ObjectStore, ObjectStoreError};

impl From<io::Error> for ObjectStoreError {
    fn from(err: io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::NotFound => ObjectStoreError::KeyNotFound(err.into()),
            kind => ObjectStoreError::Other {
                is_retriable: matches!(kind, io::ErrorKind::Interrupted | io::ErrorKind::TimedOut),
                source: err.into(),
            },
        }
    }
}

/// [`ObjectStore`] implementation storing objects as files in a local filesystem. Mostly useful for local testing.
#[derive(Debug)]
pub struct FileBackedObjectStore {
    base_dir: PathBuf,
}

impl FileBackedObjectStore {
    /// Creates a new file-backed store with its root at the specified path.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors.
    pub async fn new(base_dir: PathBuf) -> Result<Self, ObjectStoreError> {
        for bucket in &[
            Bucket::ProverJobs,
            Bucket::WitnessInput,
            Bucket::LeafAggregationWitnessJobs,
            Bucket::NodeAggregationWitnessJobs,
            Bucket::SchedulerWitnessJobs,
            Bucket::ProverJobsFri,
            Bucket::LeafAggregationWitnessJobsFri,
            Bucket::NodeAggregationWitnessJobsFri,
            Bucket::SchedulerWitnessJobsFri,
            Bucket::ProofsFri,
            Bucket::StorageSnapshot,
            Bucket::VmDumps,
            Bucket::CommitBlocksCache,
            Bucket::LocalBlobs,
        ] {
            let bucket_path = base_dir.join(bucket.to_string());
            fs::create_dir_all(&bucket_path).await?;
        }
        Ok(FileBackedObjectStore { base_dir })
    }

    fn filename(&self, bucket: Bucket, key: &str) -> PathBuf {
        self.base_dir.join(format!("{bucket}/{key}"))
    }
}

#[async_trait]
impl ObjectStore for FileBackedObjectStore {
    async fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        let filename = self.filename(bucket, key);
        fs::read(filename).await.map_err(From::from)
    }

    async fn put_raw(
        &self,
        bucket: Bucket,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), ObjectStoreError> {
        let filename = self.filename(bucket, key);
        fs::write(filename, value).await.map_err(From::from)
    }

    async fn remove_raw(&self, bucket: Bucket, key: &str) -> Result<(), ObjectStoreError> {
        let filename = self.filename(bucket, key);
        fs::remove_file(filename).await.map_err(From::from)
    }

    fn storage_prefix_raw(&self, bucket: Bucket) -> String {
        self.base_dir
            .join(bucket.to_string())
            .into_os_string()
            .into_string()
            .unwrap()
    }
}

#[cfg(test)]
mod test {
    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn test_get() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_owned();
        let object_store = FileBackedObjectStore::new(path).await.unwrap();
        let expected = vec![9, 0, 8, 9, 0, 7];
        object_store
            .put_raw(Bucket::ProverJobs, "test-key.bin", expected.clone())
            .await
            .unwrap();
        let bytes = object_store
            .get_raw(Bucket::ProverJobs, "test-key.bin")
            .await
            .unwrap();
        assert_eq!(expected, bytes, "expected didn't match");
    }

    #[tokio::test]
    async fn test_put() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_owned();
        let object_store = FileBackedObjectStore::new(path).await.unwrap();
        let bytes = vec![9, 0, 8, 9, 0, 7];
        object_store
            .put_raw(Bucket::ProverJobs, "test-key.bin", bytes)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_remove() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_owned();
        let object_store = FileBackedObjectStore::new(path).await.unwrap();
        object_store
            .put_raw(Bucket::ProverJobs, "test-key.bin", vec![0, 1])
            .await
            .unwrap();
        object_store
            .remove_raw(Bucket::ProverJobs, "test-key.bin")
            .await
            .unwrap();
    }
}
