use zksync_prover_keystore::keystore::Keystore;
use zksync_types::L1BatchId;

#[derive(Debug, Clone)]
pub struct ProofFriCompressorMetadata {
    pub l1_batch_id: L1BatchId,
    pub is_fflonk: bool,
    pub keystore: Keystore,
}

impl ProofFriCompressorMetadata {
    pub fn new(
        l1_batch_id: L1BatchId,
        is_fflonk: bool,
        keystore: Keystore,
    ) -> Self {
        Self {
            l1_batch_id,
            is_fflonk,
            keystore,
        }
    }
}
