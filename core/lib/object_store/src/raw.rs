use std::{error, fmt};

use async_trait::async_trait;

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
    ProofsTee,
    StorageSnapshot,
    TeeVerifierInput,
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
            Self::ProofsTee => "proofs_tee",
            Self::StorageSnapshot => "storage_logs_snapshots",
            Self::TeeVerifierInput => "tee_verifier_inputs",
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
#[non_exhaustive]
pub enum ObjectStoreError {
    /// Object store initialization failed.
    Initialization {
        source: BoxedError,
        is_transient: bool,
    },
    /// An object with the specified key is not found.
    KeyNotFound(BoxedError),
    /// Object (de)serialization failed.
    Serialization(BoxedError),
    /// Other error has occurred when accessing the store (e.g., a network error).
    Other {
        source: BoxedError,
        is_transient: bool,
    },
}

impl ObjectStoreError {
    /// Gives a best-effort estimate whether this error is transient.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Initialization { is_transient, .. } | Self::Other { is_transient, .. } => {
                *is_transient
            }
            Self::KeyNotFound(_) | Self::Serialization(_) => false,
        }
    }
}

impl fmt::Display for ObjectStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initialization {
                source,
                is_transient,
            } => {
                let kind = if *is_transient { "transient" } else { "fatal" };
                write!(
                    formatter,
                    "{kind} error initializing object store: {source}"
                )
            }
            Self::KeyNotFound(err) => write!(formatter, "key not found: {err}"),
            Self::Serialization(err) => write!(formatter, "serialization error: {err}"),
            Self::Other {
                source,
                is_transient,
            } => {
                let kind = if *is_transient { "transient" } else { "fatal" };
                write!(formatter, "{kind} error accessing object store: {source}")
            }
        }
    }
}

impl error::Error for ObjectStoreError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Initialization { source, .. } | Self::Other { source, .. } => {
                Some(source.as_ref())
            }
            Self::KeyNotFound(err) | Self::Serialization(err) => Some(err.as_ref()),
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
pub trait ObjectStore: 'static + fmt::Debug + Send + Sync {
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

    fn storage_prefix_raw(&self, bucket: Bucket) -> String;
}
