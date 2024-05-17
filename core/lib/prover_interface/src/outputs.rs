use core::fmt;

use circuit_sequencer_api_1_5_0::proof::FinalProof;
use serde::{Deserialize, Serialize};
use zksync_object_store::{serialize_using_bincode, Bucket, StoredObject};
use zksync_types::L1BatchNumber;

/// A "final" proof that can be sent to the L1 contract.
#[derive(Clone, Serialize, Deserialize)]
pub struct L1BatchProofForL1 {
    pub aggregation_result_coords: [[u8; 32]; 4],
    pub scheduler_proof: FinalProof,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct L1BatchTeeProofForL1 {
    // TODO revisit what else is needed here
    signature: Vec<u8>,
}

impl fmt::Debug for L1BatchProofForL1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("L1BatchProofForL1")
            .field("aggregation_result_coords", &self.aggregation_result_coords)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for L1BatchTeeProofForL1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("L1BatchTeeProofForL1")
            .finish_non_exhaustive()
    }
}

impl StoredObject for L1BatchProofForL1 {
    const BUCKET: Bucket = Bucket::ProofsFri;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("l1_batch_proof_{key}.bin")
    }

    serialize_using_bincode!();
}

impl StoredObject for L1BatchTeeProofForL1 {
    const BUCKET: Bucket = Bucket::ProofsTee;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("l1_batch_tee_proof_{key}.bin")
    }

    serialize_using_bincode!();
}
