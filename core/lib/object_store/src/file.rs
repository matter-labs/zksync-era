use std::{fmt::Debug, fs, io};

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
    pub fn new(base_dir: String) -> Self {
        for bucket in &[
            Bucket::ProverJobs,
            Bucket::WitnessInput,
            Bucket::LeafAggregationWitnessJobs,
            Bucket::NodeAggregationWitnessJobs,
            Bucket::SchedulerWitnessJobs,
        ] {
            fs::create_dir_all(format!("{base_dir}/{bucket}")).expect("failed creating bucket");
        }
        FileBackedObjectStore { base_dir }
    }

    fn filename(&self, bucket: Bucket, key: &str) -> String {
        format!("{}/{bucket}/{key}", self.base_dir)
    }
}

impl ObjectStore for FileBackedObjectStore {
    fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        let filename = self.filename(bucket, key);
        fs::read(filename).map_err(From::from)
    }

    fn put_raw(&self, bucket: Bucket, key: &str, value: Vec<u8>) -> Result<(), ObjectStoreError> {
        let filename = self.filename(bucket, key);
        fs::write(filename, value).map_err(From::from)
    }

    fn remove_raw(&self, bucket: Bucket, key: &str) -> Result<(), ObjectStoreError> {
        let filename = self.filename(bucket, key);
        fs::remove_file(filename).map_err(From::from)
    }
}

#[cfg(test)]
mod test {
    use tempdir::TempDir;

    use super::*;

    #[test]
    fn test_get() {
        let dir = TempDir::new("test-data").unwrap();
        let path = dir.into_path().into_os_string().into_string().unwrap();
        let object_store = FileBackedObjectStore::new(path);
        let expected = vec![9, 0, 8, 9, 0, 7];
        let result = object_store.put_raw(Bucket::ProverJobs, "test-key.bin", expected.clone());
        assert!(result.is_ok(), "result must be OK");
        let bytes = object_store
            .get_raw(Bucket::ProverJobs, "test-key.bin")
            .unwrap();
        assert_eq!(expected, bytes, "expected didn't match");
    }

    #[test]
    fn test_put() {
        let dir = TempDir::new("test-data").unwrap();
        let path = dir.into_path().into_os_string().into_string().unwrap();
        let object_store = FileBackedObjectStore::new(path);
        let bytes = vec![9, 0, 8, 9, 0, 7];
        let result = object_store.put_raw(Bucket::ProverJobs, "test-key.bin", bytes);
        assert!(result.is_ok(), "result must be OK");
    }

    #[test]
    fn test_remove() {
        let dir = TempDir::new("test-data").unwrap();
        let path = dir.into_path().into_os_string().into_string().unwrap();
        let object_store = FileBackedObjectStore::new(path);
        let result = object_store.put_raw(Bucket::ProverJobs, "test-key.bin", vec![0, 1]);
        assert!(result.is_ok(), "result must be OK");
        let result = object_store.remove_raw(Bucket::ProverJobs, "test-key.bin");
        assert!(result.is_ok(), "result must be OK");
    }
}
