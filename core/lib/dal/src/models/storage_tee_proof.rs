use chrono::NaiveDateTime;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageTeeProof {
    pub pubkey: Option<Vec<u8>>,
    pub signature: Option<Vec<u8>>,
    pub proof: Option<Vec<u8>>,
    pub updated_at: NaiveDateTime,
    pub attestation: Option<Vec<u8>>,
}
