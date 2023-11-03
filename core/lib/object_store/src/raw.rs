use async_trait::async_trait;

use std::{error, fmt, sync::Arc};

use crate::{file::FileBackedObjectStore, gcs::GoogleCloudStorage, mock::MockStore};
use zksync_config::configs::object_store::{ObjectStoreConfig, ObjectStoreMode};

/// Bucket for [`ObjectStore`] in which objects can be placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Bucket {
    ProverJobs,
    WitnessInput,
    LeafAggregationWitnessJobs,
    NodeAggregationWitnessJobs,
    SchedulerWitnessJobs,
    ProverJobsFri,
    LeafAggregationWitnessJobsFri,
    NodeAggregationWitnessJobsFri,
    SchedulerWitnessJobsFri,
    ProofsFri,
}

impl Bucket {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ProverJobs => "prover_jobs",
            Self::WitnessInput => "witness_inputs",
            Self::LeafAggregationWitnessJobs => "leaf_aggregation_witness_jobs",
            Self::NodeAggregationWitnessJobs => "node_aggregation_witness_jobs",
            Self::SchedulerWitnessJobs => "scheduler_witness_jobs",
            Self::ProverJobsFri => "prover_jobs_fri",
            Self::LeafAggregationWitnessJobsFri => "leaf_aggregation_witness_jobs_fri",
            Self::NodeAggregationWitnessJobsFri => "node_aggregation_witness_jobs_fri",
            Self::SchedulerWitnessJobsFri => "scheduler_witness_jobs_fri",
            Self::ProofsFri => "proofs_fri",
        }
    }
}

impl fmt::Display for Bucket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Thread-safe boxed error.
pub type BoxedError = Box<dyn error::Error + Send + Sync>;

/// Errors during [`ObjectStore`] operations.
#[derive(Debug)]
pub enum ObjectStoreError {
    /// An object with the specified key is not found.
    KeyNotFound(BoxedError),
    /// Object (de)serialization failed.
    Serialization(BoxedError),
    /// Other error has occurred when accessing the store (e.g., a network error).
    Other(BoxedError),
}

impl fmt::Display for ObjectStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeyNotFound(err) => write!(formatter, "key not found: {err}"),
            Self::Serialization(err) => write!(formatter, "serialization error: {err}"),
            Self::Other(err) => write!(formatter, "other error: {err}"),
        }
    }
}

impl error::Error for ObjectStoreError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::KeyNotFound(err) | Self::Serialization(err) | Self::Other(err) => {
                Some(err.as_ref())
            }
        }
    }
}

/// Functionality to fetch and store byte blobs from an object store (AWS S3, Google Cloud Storage,
/// Azure Blobstore etc).
///
/// The methods of this trait are low-level. Prefer implementing [`StoredObject`] for the store
/// object and using `get()` / `put()` methods in `dyn ObjectStore`.
///
/// [`StoredObject`]: crate::StoredObject
#[async_trait]
pub trait ObjectStore: fmt::Debug + Send + Sync {
    /// Fetches the value for the given key from the given bucket if it exists.
    ///
    /// # Errors
    ///
    /// Returns an error if an object with the `key` does not exist or cannot be accessed.
    async fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError>;

    /// Stores the value associating it with the key into the given bucket.
    /// If the key already exists, the value is replaced.
    ///
    /// # Errors
    ///
    /// Returns an error if the insertion / replacement operation fails.
    async fn put_raw(
        &self,
        bucket: Bucket,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), ObjectStoreError>;

    /// Removes the value associated with the key from the given bucket if it exists.
    ///
    /// # Errors
    ///
    /// Returns an error if removal fails.
    async fn remove_raw(&self, bucket: Bucket, key: &str) -> Result<(), ObjectStoreError>;
}

#[async_trait]
impl<T: ObjectStore + ?Sized> ObjectStore for Arc<T> {
    async fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        (**self).get_raw(bucket, key).await
    }

    async fn put_raw(
        &self,
        bucket: Bucket,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), ObjectStoreError> {
        (**self).put_raw(bucket, key, value).await
    }

    async fn remove_raw(&self, bucket: Bucket, key: &str) -> Result<(), ObjectStoreError> {
        (**self).remove_raw(bucket, key).await
    }
}

#[derive(Debug)]
enum ObjectStoreOrigin {
    Config(ObjectStoreConfig),
    Mock(Arc<MockStore>),
}

/// Factory of [`ObjectStore`]s.
#[derive(Debug)]
pub struct ObjectStoreFactory {
    origin: ObjectStoreOrigin,
}

impl ObjectStoreFactory {
    /// Creates an object store factory based on the provided `config`.
    ///
    /// # Panics
    ///
    /// If the GCS-backed implementation is configured, this constructor will panic if called
    /// outside the Tokio runtime.
    pub fn new(config: ObjectStoreConfig) -> Self {
        Self {
            origin: ObjectStoreOrigin::Config(config),
        }
    }

    /// Creates an object store factory with a mock in-memory store.
    /// All calls to [`Self::create_store()`] will return the same store; thus, the testing code
    /// can use [`ObjectStore`] methods for assertions.
    pub fn mock() -> Self {
        Self {
            origin: ObjectStoreOrigin::Mock(Arc::new(MockStore::default())),
        }
    }

    /// Creates an [`ObjectStore`].
    pub async fn create_store(&self) -> Box<dyn ObjectStore> {
        match &self.origin {
            ObjectStoreOrigin::Config(config) => Self::create_from_config(config).await,
            ObjectStoreOrigin::Mock(store) => Box::new(Arc::clone(store)),
        }
    }

    async fn create_from_config(config: &ObjectStoreConfig) -> Box<dyn ObjectStore> {
        let gcs_credential_file_path = match config.mode {
            ObjectStoreMode::GCSWithCredentialFile => Some(config.gcs_credential_file_path.clone()),
            _ => None,
        };
        match config.mode {
            ObjectStoreMode::GCS => {
                tracing::trace!(
                    "Initialized GoogleCloudStorage Object store without credential file"
                );
                let store = GoogleCloudStorage::new(
                    gcs_credential_file_path,
                    config.bucket_base_url.clone(),
                    config.max_retries,
                )
                .await;
                Box::new(store)
            }
            ObjectStoreMode::GCSWithCredentialFile => {
                tracing::trace!("Initialized GoogleCloudStorage Object store with credential file");
                let store = GoogleCloudStorage::new(
                    gcs_credential_file_path,
                    config.bucket_base_url.clone(),
                    config.max_retries,
                )
                .await;
                Box::new(store)
            }
            ObjectStoreMode::FileBacked => {
                tracing::trace!("Initialized FileBacked Object store");
                let store = FileBackedObjectStore::new(config.file_backed_base_path.clone()).await;
                Box::new(store)
            }
        }
    }
}
