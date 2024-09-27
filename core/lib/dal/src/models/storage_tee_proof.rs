use chrono::NaiveDateTime;
use zksync_types::{tee_types::LockedBatch, L1BatchNumber};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageTeeProof {
    pub pubkey: Option<Vec<u8>>,
    pub signature: Option<Vec<u8>>,
    pub proof: Option<Vec<u8>>,
    pub updated_at: NaiveDateTime,
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
            created_at: tx.created_at,
        }
    }
}
