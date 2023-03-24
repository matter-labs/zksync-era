use std::fmt::Debug;
use std::fs;
use std::fs::File;
use std::io::{ErrorKind, Read, Write};

use crate::object_store::{
    ObjectStore, ObjectStoreError, LEAF_AGGREGATION_WITNESS_JOBS_BUCKET_PATH,
    NODE_AGGREGATION_WITNESS_JOBS_BUCKET_PATH, PROVER_JOBS_BUCKET_PATH,
    SCHEDULER_WITNESS_JOBS_BUCKET_PATH, WITNESS_INPUT_BUCKET_PATH,
};

impl From<std::io::Error> for ObjectStoreError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            ErrorKind::NotFound => ObjectStoreError::KeyNotFound(err.to_string()),
            _ => ObjectStoreError::Other(err.to_string()),
        }
    }
}

#[derive(Debug)]
pub struct FileBackedObjectStore {
    base_dir: String,
}

impl FileBackedObjectStore {
    pub fn new(base_dir: String) -> Self {
        for bucket in &[
            PROVER_JOBS_BUCKET_PATH,
            WITNESS_INPUT_BUCKET_PATH,
            LEAF_AGGREGATION_WITNESS_JOBS_BUCKET_PATH,
            NODE_AGGREGATION_WITNESS_JOBS_BUCKET_PATH,
            SCHEDULER_WITNESS_JOBS_BUCKET_PATH,
        ] {
            fs::create_dir_all(format!("{}/{}", base_dir, bucket)).expect("failed creating bucket");
        }
        FileBackedObjectStore { base_dir }
    }

    fn filename(&self, bucket: &'static str, key: String) -> String {
        format!("{}/{}/{}", self.base_dir, bucket, key)
    }
}

impl ObjectStore for FileBackedObjectStore {
    type Bucket = &'static str;
    type Key = String;
    type Value = Vec<u8>;

    fn get_store_type(&self) -> &'static str {
        "FileBackedStore"
    }

    fn get(&self, bucket: Self::Bucket, key: Self::Key) -> Result<Self::Value, ObjectStoreError> {
        let filename = self.filename(bucket, key);
        let mut file = File::open(filename)?;
        let mut buffer = Vec::<u8>::new();
        file.read_to_end(&mut buffer)?;
        Ok(buffer)
    }

    fn put(
        &mut self,
        bucket: Self::Bucket,
        key: Self::Key,
        value: Self::Value,
    ) -> Result<(), ObjectStoreError> {
        let filename = self.filename(bucket, key);
        let mut file = File::create(filename)?;
        file.write_all(&value)?;
        Ok(())
    }

    fn remove(&mut self, bucket: Self::Bucket, key: Self::Key) -> Result<(), ObjectStoreError> {
        let filename = self.filename(bucket, key);
        fs::remove_file(filename)?;
        Ok(())
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
        let mut object_store = FileBackedObjectStore::new(path);
        let expected = vec![9, 0, 8, 9, 0, 7];
        let result = object_store.put(
            PROVER_JOBS_BUCKET_PATH,
            "test-key.bin".to_string(),
            expected.clone(),
        );
        assert!(result.is_ok(), "result must be OK");
        let bytes = object_store
            .get(PROVER_JOBS_BUCKET_PATH, "test-key.bin".to_string())
            .unwrap();
        assert_eq!(expected, bytes, "expected didn't match");
    }

    #[test]
    fn test_put() {
        let dir = TempDir::new("test-data").unwrap();
        let path = dir.into_path().into_os_string().into_string().unwrap();
        let mut object_store = FileBackedObjectStore::new(path);
        let bytes = vec![9, 0, 8, 9, 0, 7];
        let result = object_store.put(PROVER_JOBS_BUCKET_PATH, "test-key.bin".to_string(), bytes);
        assert!(result.is_ok(), "result must be OK");
    }

    #[test]
    fn test_remove() {
        let dir = TempDir::new("test-data").unwrap();
        let path = dir.into_path().into_os_string().into_string().unwrap();
        let mut object_store = FileBackedObjectStore::new(path);
        let result = object_store.put(
            PROVER_JOBS_BUCKET_PATH,
            "test-key.bin".to_string(),
            vec![0, 1],
        );
        assert!(result.is_ok(), "result must be OK");
        let result = object_store.remove(PROVER_JOBS_BUCKET_PATH, "test-key.bin".to_string());
        assert!(result.is_ok(), "result must be OK");
    }
}
