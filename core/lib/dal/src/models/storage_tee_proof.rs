use chrono::{DateTime, NaiveDateTime, Utc};
use zksync_types::L1BatchNumber;

use crate::tee_proof_generation_dal::LockedBatch;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageTeeProof {
    pub pubkey: Option<Vec<u8>>,
    pub signature: Option<Vec<u8>>,
    pub proof: Option<Vec<u8>>,
    pub updated_at: NaiveDateTime,
    pub status: String,
    pub attestation: Option<Vec<u8>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageLockedBatch {
    pub l1_batch_number: i64,
    pub created_at: NaiveDateTime,
}

impl From<StorageLockedBatch> for LockedBatch {
    fn from(tx: StorageLockedBatch) -> LockedBatch {
        LockedBatch {
            l1_batch_number: L1BatchNumber::from(tx.l1_batch_number as u32),
            created_at: DateTime::<Utc>::from_naive_utc_and_offset(tx.created_at, Utc),
        }
    }
}
