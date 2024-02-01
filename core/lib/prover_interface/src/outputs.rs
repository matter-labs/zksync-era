use core::fmt;

use serde::{Deserialize, Serialize};
use zkevm_test_harness::{
    abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit,
    bellman::{bn256::Bn256, plonk::better_better_cs::proof::Proof},
    witness::oracle::VmWitnessOracle,
};
use zksync_object_store::{serialize_using_bincode, Bucket, StoredObject};
use zksync_types::L1BatchNumber;

/// The only type of proof utilized by the core subsystem: a "final" proof that can be sent
/// to the L1 contract.
#[derive(Clone, Serialize, Deserialize)]
pub struct L1BatchProofForL1 {
    pub aggregation_result_coords: [[u8; 32]; 4],
    pub scheduler_proof: Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
}

impl fmt::Debug for L1BatchProofForL1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("L1BatchProofForL1")
            .field("aggregation_result_coords", &self.aggregation_result_coords)
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
