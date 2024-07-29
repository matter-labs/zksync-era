use serde::{Deserialize, Serialize}; // TODO needed?
use zksync_types::{tee_types::TeeType, L1BatchNumber};

/// Represents a proof generated within a TEE enclave
/// TODO move it to core/lib/types/src/api/mod.rs and call TeeProof?
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageTeeProof {
    // batch number for which the proof was generated
    pub l1_batch_number: L1BatchNumber,
    // type of TEE used for attestation
    pub tee_type: Option<TeeType>,
    // pubkey used for signature verification; each key pair is attested by the TEE attestation
    // stored in the db
    pub pubkey: Option<Vec<u8>>,
    // signature generated within the TEE enclave, using the privkey corresponding to the pubkey
    pub signature: Option<Vec<u8>>,
    // data that was signed
    pub proof: Option<Vec<u8>>,
    // attestation quote generated within the TEE enclave
    pub attestation: Option<Vec<u8>>,
    // timestamp when the proof was generated
    pub proved_at: chrono::DateTime<chrono::Utc>,
}

/// TODO rename it TeeProof once StorageTeeProof is moved to api/mod.rs?
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TmpStorageTeeProof {
    #[allow(dead_code)]
    pub id: i64,
    pub pubkey: Option<Vec<u8>>,
    pub signature: Option<Vec<u8>>,
    pub proof: Option<Vec<u8>>,
    pub proved_at: chrono::DateTime<chrono::Utc>,
    pub attestation: Option<Vec<u8>>,
}
