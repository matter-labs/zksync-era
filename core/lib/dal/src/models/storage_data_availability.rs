use chrono::NaiveDateTime;
use zksync_types::{pubdata_da::DataAvailabilityBlob, L1BatchNumber};

/// Represents a blob in the data availability layer.
#[derive(Debug, Clone)]
pub(crate) struct StorageDABlob {
    pub l1_batch_number: i64,
    pub blob_id: String,
    pub inclusion_data: Option<Vec<u8>>,
    pub sent_at: NaiveDateTime,
}

impl From<StorageDABlob> for DataAvailabilityBlob {
    fn from(blob: StorageDABlob) -> DataAvailabilityBlob {
        DataAvailabilityBlob {
            l1_batch_number: L1BatchNumber(blob.l1_batch_number as u32),
            blob_id: blob.blob_id,
            inclusion_data: blob.inclusion_data,
            sent_at: blob.sent_at.and_utc(),
        }
    }
}

/// A small struct used to store a batch and its data availability, which are retrieved from the database.
#[derive(Debug)]
pub struct L1BatchDA {
    pub pubdata: Vec<u8>,
    pub l1_batch_number: L1BatchNumber,
}
