use std::error;
use std::fmt::Debug;
use std::str::FromStr;

pub use cloud_storage::Error;
use zksync_config::ObjectStoreConfig;

use crate::file_backed_object_store::FileBackedObjectStore;
use crate::gcs_object_store::GoogleCloudStorage;

pub const PROVER_JOBS_BUCKET_PATH: &str = "prover_jobs";
pub const WITNESS_INPUT_BUCKET_PATH: &str = "witness_inputs";
pub const LEAF_AGGREGATION_WITNESS_JOBS_BUCKET_PATH: &str = "leaf_aggregation_witness_jobs";
pub const NODE_AGGREGATION_WITNESS_JOBS_BUCKET_PATH: &str = "node_aggregation_witness_jobs";
pub const SCHEDULER_WITNESS_JOBS_BUCKET_PATH: &str = "scheduler_witness_jobs";

/// Trait to fetch and store BLOB's from an object store(S3, Google Cloud Storage, Azure Blobstore etc).
pub trait ObjectStore: Debug + Send + Sync {
    type Bucket: Debug;
    type Key: Debug;
    type Value;
    type Error;

    fn get_store_type(&self) -> &'static str;

    /// Fetches the value for the given key from the given bucket if it exists otherwise returns Error.
    fn get(&self, bucket: Self::Bucket, key: Self::Key) -> Result<Self::Value, Self::Error>;

    /// Stores the value associating it with the key into the given bucket, if the key already exist then the value is replaced.
    fn put(
        &mut self,
        bucket: Self::Bucket,
        key: Self::Key,
        value: Self::Value,
    ) -> Result<(), Self::Error>;

    /// Removes the value associated with the key from the given bucket if it exist.
    fn remove(&mut self, bucket: Self::Bucket, key: Self::Key) -> Result<(), Self::Error>;
}

pub type DynamicObjectStore = Box<
    dyn ObjectStore<
        Bucket = &'static str,
        Error = Box<dyn error::Error>,
        Key = String,
        Value = Vec<u8>,
    >,
>;

#[derive(Debug, Eq, PartialEq)]
pub enum ObjectStoreMode {
    GCS,
    FileBacked,
}

impl FromStr for ObjectStoreMode {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "GCS" => Ok(ObjectStoreMode::GCS),
            "FileBacked" => Ok(ObjectStoreMode::FileBacked),
            _ => Err(format!("Unknown ObjectStoreMode type: {}", input)),
        }
    }
}

pub fn create_object_store(
    mode: ObjectStoreMode,
    file_backed_base_path: String,
) -> DynamicObjectStore {
    match mode {
        ObjectStoreMode::GCS => {
            vlog::trace!("Initialized GoogleCloudStorage Object store");
            Box::new(GoogleCloudStorage::new())
        }
        ObjectStoreMode::FileBacked => {
            vlog::trace!("Initialized FileBacked Object store");
            Box::new(FileBackedObjectStore::new(file_backed_base_path))
        }
    }
}

pub fn create_object_store_from_env() -> DynamicObjectStore {
    let config = ObjectStoreConfig::from_env();
    let mode = ObjectStoreMode::from_str(&config.mode).unwrap();
    create_object_store(mode, config.file_backed_base_path)
}
