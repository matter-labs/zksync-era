pub mod types;

use std::fmt;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use types::{DAError, DispatchResponse, InclusionData};

use crate::types::{ClientType, FinalityResponse};

/// Trait that defines the interface for the data availability layer clients.
#[async_trait]
pub trait DataAvailabilityClient: Sync + Send + fmt::Debug {
    /// Dispatches a blob to the data availability layer.
    async fn dispatch_blob(
        &self,
        batch_number: u32,
        data: Vec<u8>,
    ) -> Result<DispatchResponse, DAError>;

    /// Ensures the finality of a blob.
    async fn ensure_finality(
        &self,
        dispatch_request_id: String,
        dispatched_at: DateTime<Utc>,
    ) -> Result<Option<FinalityResponse>, DAError>;

    /// Fetches the inclusion data for a given blob_id.
    async fn get_inclusion_data(&self, blob_id: &str) -> Result<Option<InclusionData>, DAError>;

    /// Clones the client and wraps it in a Box.
    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient>;

    /// Returns the maximum size of the blob (in bytes) that can be dispatched. None means no limit.
    fn blob_size_limit(&self) -> Option<usize>;

    /// Returns the name of the client implementation.
    fn client_type(&self) -> ClientType;

    /// Returns the balance of the operator account.
    async fn balance(&self) -> Result<u64, DAError>;
}

impl Clone for Box<dyn DataAvailabilityClient> {
    fn clone(&self) -> Box<dyn DataAvailabilityClient> {
        self.clone_boxed()
    }
}
