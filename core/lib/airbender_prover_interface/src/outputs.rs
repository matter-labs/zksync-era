use core::fmt;

use serde::{Deserialize, Serialize};
use serde_with::{hex::Hex, serde_as};
use zksync_object_store::{serialize_using_bincode, Bucket, StoredObject};
use zksync_types::L1BatchNumber;

/// A "final" Airbender proof that can be sent to the L1 contract.
#[serde_as]
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct L1BatchAirbenderProofForL1 {
    #[serde_as(as = "Hex")]
    pub proof: Vec<u8>,
}

impl fmt::Debug for L1BatchAirbenderProofForL1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("L1BatchAirbenderProofForL1")
            .finish_non_exhaustive()
    }
}

impl StoredObject for L1BatchAirbenderProofForL1 {
    const BUCKET: Bucket = Bucket::ProofsAirbender;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("l1_batch_airbender_proof_{key}.bin")
    }

    serialize_using_bincode!();
}
