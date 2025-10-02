use chrono::{DateTime, NaiveDateTime, Utc};
use zksync_types::{
    l2_to_l1_log::L2ToL1Log,
    pubdata_da::{DataAvailabilityBlob, DataAvailabilityDetails},
    Address, L1BatchNumber,
};

/// Represents a blob in the data availability layer.
#[derive(Debug, Clone)]
pub(crate) struct StorageDABlob {
    pub l1_batch_number: i64,
    pub dispatch_request_id: String,
    pub blob_id: Option<String>,
    pub inclusion_data: Option<Vec<u8>>,
    pub sent_at: NaiveDateTime,
}

impl From<StorageDABlob> for DataAvailabilityBlob {
    fn from(blob: StorageDABlob) -> DataAvailabilityBlob {
        DataAvailabilityBlob {
            l1_batch_number: L1BatchNumber(blob.l1_batch_number as u32),
            dispatch_request_id: blob.dispatch_request_id,
            blob_id: blob.blob_id,
            inclusion_data: blob.inclusion_data,
            sent_at: blob.sent_at.and_utc(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StorageDADetails {
    pub blob_id: String,
    pub client_type: Option<String>,
    pub inclusion_data: Option<Vec<u8>>,
    pub sent_at: NaiveDateTime,
    pub l2_da_validator_address: Option<Vec<u8>>,
}

impl From<StorageDADetails> for DataAvailabilityDetails {
    fn from(row: StorageDADetails) -> DataAvailabilityDetails {
        DataAvailabilityDetails {
            // safe to unwrap because query is guaranteed to return a non-null blob_id
            blob_id: row.blob_id,
            // safe to unwrap because the value in the database is assumed to be always correct
            pubdata_type: row.client_type.map(|t| t.parse().unwrap()),
            inclusion_data: row.inclusion_data,
            sent_at: row.sent_at.and_utc(),
            l2_da_validator: row
                .l2_da_validator_address
                .map(|addr| Address::from_slice(addr.as_slice())),
        }
    }
}

/// A small struct used to store a batch and its data availability, which are retrieved from the database.
#[derive(Debug)]
pub struct L1BatchDA {
    pub pubdata: Vec<u8>,
    pub l1_batch_number: L1BatchNumber,
    pub system_logs: Vec<L2ToL1Log>,
    pub sealed_at: DateTime<Utc>,
}
