use async_trait::async_trait;
use tokio::{fs, io};

use std::fmt::Debug;

use crate::raw::{Bucket, ObjectStore, ObjectStoreError};

impl From<io::Error> for ObjectStoreError {
    fn from(err: io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::NotFound => ObjectStoreError::KeyNotFound(err.into()),
            _ => ObjectStoreError::Other(err.into()),
        }
    }
}

#[derive(Debug)]
pub(crate) struct FileBackedObjectStore {
    base_dir: String,
}

impl FileBackedObjectStore {
    pub async fn new(base_dir: String) -> Self {
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
        ] {
            let bucket_path = format!("{base_dir}/{bucket}");
            fs::create_dir_all(&bucket_path)
                .await
                .unwrap_or_else(|err| {
                    panic!("failed creating bucket `{bucket_path}`: {err}");
                });
        }
        FileBackedObjectStore { base_dir }
    }

    fn filename(&self, bucket: Bucket, key: &str) -> String {
        format!("{}/{bucket}/{key}", self.base_dir)
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
}

#[cfg(test)]
mod test {
    use tempdir::TempDir;

    use super::*;

    #[tokio::test]
    async fn test_get() {
        let dir = TempDir::new("test-data").unwrap();
        let path = dir.into_path().into_os_string().into_string().unwrap();
        let object_store = FileBackedObjectStore::new(path).await;
        let expected = vec![9, 0, 8, 9, 0, 7];
        let result = object_store
            .put_raw(Bucket::ProverJobs, "test-key.bin", expected.clone())
            .await;
        assert!(result.is_ok(), "result must be OK");
        let bytes = object_store
            .get_raw(Bucket::ProverJobs, "test-key.bin")
            .await
            .unwrap();
        assert_eq!(expected, bytes, "expected didn't match");
    }

    #[tokio::test]
    async fn test_put() {
        let dir = TempDir::new("test-data").unwrap();
        let path = dir.into_path().into_os_string().into_string().unwrap();
        let object_store = FileBackedObjectStore::new(path).await;
        let bytes = vec![9, 0, 8, 9, 0, 7];
        let result = object_store
            .put_raw(Bucket::ProverJobs, "test-key.bin", bytes)
            .await;
        assert!(result.is_ok(), "result must be OK");
    }

    #[tokio::test]
    async fn test_remove() {
        let dir = TempDir::new("test-data").unwrap();
        let path = dir.into_path().into_os_string().into_string().unwrap();
        let object_store = FileBackedObjectStore::new(path).await;
        let result = object_store
            .put_raw(Bucket::ProverJobs, "test-key.bin", vec![0, 1])
            .await;
        assert!(result.is_ok(), "result must be OK");
        let result = object_store
            .remove_raw(Bucket::ProverJobs, "test-key.bin")
            .await;
        assert!(result.is_ok(), "result must be OK");
    }
}
