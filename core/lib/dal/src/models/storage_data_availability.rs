use chrono::{DateTime, NaiveDateTime, Utc};
use zksync_types::{
    l2_to_l1_log::L2ToL1Log, pubdata_da::DataAvailabilityBlob, Address, L1BatchNumber, H256,
};

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
    pub system_logs: Vec<L2ToL1Log>,
    pub sealed_at: DateTime<Utc>,
}

impl L1BatchDA {
    pub fn l2_da_validator_address(&self) -> Address {
        let value = self
            .system_logs
            .iter()
            .find(|log| {
                log.key
                    == H256::from_low_u64_be(u64::from(
                        zksync_system_constants::L2_DA_VALIDATOR_OUTPUT_HASH_KEY,
                    ))
            })
            .unwrap() // The log is guaranteed to be present
            .value;

        value.into()
    }
}
