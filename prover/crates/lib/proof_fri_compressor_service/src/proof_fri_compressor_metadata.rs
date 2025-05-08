use zksync_types::{protocol_version::L1VerifierConfig, L1BatchId};

#[derive(Debug, Clone)]
pub struct ProofFriCompressorMetadata {
    pub l1_batch_id: L1BatchId,
    pub l1_verifier_config: L1VerifierConfig,
}

impl ProofFriCompressorMetadata {
    pub fn new(l1_batch_id: L1BatchId, l1_verifier_config: L1VerifierConfig) -> Self {
        Self {
            l1_batch_id,
            l1_verifier_config,
        }
    }
}
