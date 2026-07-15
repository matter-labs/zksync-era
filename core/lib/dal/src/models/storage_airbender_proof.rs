use chrono::{DateTime, NaiveDateTime, Utc};
use zksync_types::{
    protocol_version::{ProtocolSemanticVersion, VersionPatch},
    L1BatchNumber,
};

use crate::airbender_proof_generation_dal::LockedBatch;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageAirbenderProof {
    pub proof_blob_url: Option<String>,
    pub updated_at: NaiveDateTime,
    pub status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageAirbenderSnarkProof {
    pub snark_proof_blob_url: Option<String>,
    pub updated_at: NaiveDateTime,
    pub status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageLockedBatch {
    pub l1_batch_number: i64,
    pub created_at: NaiveDateTime,
    pub protocol_version: i32,
    pub protocol_version_patch: i32,
}

impl From<StorageLockedBatch> for LockedBatch {
    fn from(tx: StorageLockedBatch) -> LockedBatch {
        LockedBatch {
            l1_batch_number: L1BatchNumber::from(tx.l1_batch_number as u32),
            created_at: DateTime::<Utc>::from_naive_utc_and_offset(tx.created_at, Utc),
            protocol_version: ProtocolSemanticVersion {
                minor: (tx.protocol_version as u16).try_into().unwrap(),
                patch: VersionPatch(tx.protocol_version_patch as u32),
            },
        }
    }
}
