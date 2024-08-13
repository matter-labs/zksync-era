use chrono::{DateTime, Utc};
use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};
use zksync_basic_types::L1BatchNumber;
use zksync_config::configs::eth_sender::PubdataSendingMode;

/// Enum holding the current values used for DA Layers.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
#[derive(TryFromPrimitive)]
pub enum PubdataDA {
    /// Pubdata is sent to the L1 as a tx calldata.
    Calldata = 0,
    /// Pubdata is sent to L1 as EIP-4844 blobs.
    Blobs,
    /// Pubdata is sent to the external storage (GCS/DA layers) or not sent at all.
    Custom,
    /// Pubdata is sent to an L2 to be eventually relayed to L1.
    RelayedL2Calldata,
}

impl From<PubdataSendingMode> for PubdataDA {
    fn from(value: PubdataSendingMode) -> Self {
        match value {
            PubdataSendingMode::Calldata => PubdataDA::Calldata,
            PubdataSendingMode::Blobs => PubdataDA::Blobs,
            PubdataSendingMode::Custom => PubdataDA::Custom,
            PubdataSendingMode::RelayedL2Calldata => PubdataDA::RelayedL2Calldata,
        }
    }
}

/// Represents a blob in the data availability layer.
#[derive(Debug, Clone)]
pub struct DataAvailabilityBlob {
    pub l1_batch_number: L1BatchNumber,
    pub blob_id: String,
    pub inclusion_data: Option<Vec<u8>>,
    pub verify_inclusion: bool,
    pub sent_at: DateTime<Utc>,
}
