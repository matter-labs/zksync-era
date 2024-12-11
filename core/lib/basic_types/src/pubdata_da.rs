//! Types related to data availability.

use chrono::{DateTime, Utc};
use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};

use crate::L1BatchNumber;

/// Enum holding the current values used for DA Layers.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize, Serialize, TryFromPrimitive)]
pub enum PubdataSendingMode {
    /// Pubdata is sent to the L1 as a tx calldata.
    #[default]
    Calldata = 0,
    /// Pubdata is sent to L1 as EIP-4844 blobs.
    Blobs,
    /// Pubdata is sent to the external storage (GCS/DA layers) or not sent at all.
    Custom,
    /// Pubdata is sent to an L2 to be eventually relayed to L1.
    RelayedL2Calldata,
}

/// Represents a blob in the data availability layer.
#[derive(Debug, Clone)]
pub struct DataAvailabilityBlob {
    pub l1_batch_number: L1BatchNumber,
    pub blob_id: String,
    pub inclusion_data: Option<Vec<u8>>,
    pub sent_at: DateTime<Utc>,
}
