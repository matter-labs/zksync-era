//! Mock implementation of [`ObjectStore`].

use std::{collections::HashMap, sync::Mutex};

use crate::raw::{Bucket, ObjectStore, ObjectStoreError};

type BucketMap = HashMap<String, Vec<u8>>;

#[derive(Debug, Default)]
pub(crate) struct MockStore {
    inner: Mutex<HashMap<Bucket, BucketMap>>,
}

impl ObjectStore for MockStore {
    fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        let lock = self.inner.lock().expect("mock object store poisoned");
        let maybe_bytes = lock.get(&bucket).and_then(|bucket_map| bucket_map.get(key));
        maybe_bytes.cloned().ok_or_else(|| {
            let error_message = format!("missing key: {key} in bucket {bucket}");
            ObjectStoreError::KeyNotFound(error_message.into())
        })
    }

    fn put_raw(&self, bucket: Bucket, key: &str, value: Vec<u8>) -> Result<(), ObjectStoreError> {
        let mut lock = self.inner.lock().expect("mock object store poisoned");
        let bucket_map = lock.entry(bucket).or_default();
        bucket_map.insert(key.to_owned(), value);
        Ok(())
    }

    fn remove_raw(&self, bucket: Bucket, key: &str) -> Result<(), ObjectStoreError> {
        let mut lock = self.inner.lock().expect("mock object store poisoned");
        let Some(bucket_map) = lock.get_mut(&bucket) else {
            return Ok(())
        };
        bucket_map.remove(key);
        Ok(())
    }
}
