use google_cloud_default::WithAuthExt;
use google_cloud_storage::client::{Client, ClientConfig};
use std::fmt::{Debug, Display, Formatter};
use std::str::FromStr;
use std::sync::mpsc::channel;
use std::{error, thread};
use tokio::runtime::Builder;

use zksync_config::ObjectStoreConfig;

use crate::file_backed_object_store::FileBackedObjectStore;
use crate::gcs_object_store::GoogleCloudStorage;

pub const PROVER_JOBS_BUCKET_PATH: &str = "prover_jobs";
pub const WITNESS_INPUT_BUCKET_PATH: &str = "witness_inputs";
pub const LEAF_AGGREGATION_WITNESS_JOBS_BUCKET_PATH: &str = "leaf_aggregation_witness_jobs";
pub const NODE_AGGREGATION_WITNESS_JOBS_BUCKET_PATH: &str = "node_aggregation_witness_jobs";
pub const SCHEDULER_WITNESS_JOBS_BUCKET_PATH: &str = "scheduler_witness_jobs";

#[derive(Debug)]
pub enum ObjectStoreError {
    KeyNotFound(String),
    Other(String),
}

impl Display for ObjectStoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectStoreError::KeyNotFound(e) => write!(f, "Key Notfound error: {}", e),
            ObjectStoreError::Other(s) => write!(f, "Other error: {}", s),
        }
    }
}

impl error::Error for ObjectStoreError {}

/// Trait to fetch and store BLOB's from an object store(S3, Google Cloud Storage, Azure Blobstore etc).
pub trait ObjectStore: Debug + Send + Sync {
    type Bucket: Debug;
    type Key: Debug;
    type Value;

    fn get_store_type(&self) -> &'static str;

    /// Fetches the value for the given key from the given bucket if it exists otherwise returns Error.
    fn get(&self, bucket: Self::Bucket, key: Self::Key) -> Result<Self::Value, ObjectStoreError>;

    /// Stores the value associating it with the key into the given bucket, if the key already exist then the value is replaced.
    fn put(
        &mut self,
        bucket: Self::Bucket,
        key: Self::Key,
        value: Self::Value,
    ) -> Result<(), ObjectStoreError>;

    /// Removes the value associated with the key from the given bucket if it exist.
    fn remove(&mut self, bucket: Self::Bucket, key: Self::Key) -> Result<(), ObjectStoreError>;
}

pub type DynamicObjectStore =
    Box<dyn ObjectStore<Bucket = &'static str, Key = String, Value = Vec<u8>>>;

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
            let gcs_config = fetch_gcs_config();
            Box::new(GoogleCloudStorage::new(Client::new(gcs_config)))
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

fn fetch_gcs_config() -> ClientConfig {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let runtime = Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .unwrap();
        let result = runtime
            .block_on(ClientConfig::default().with_auth())
            .expect("Failed build GCS client config");
        tx.send(result).unwrap();
    });
    rx.recv().unwrap()
}
