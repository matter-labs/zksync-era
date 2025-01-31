//! Different key types for object store.

use zksync_types::{
    basic_fri_types::AggregationRound, prover_dal::FriProverJobMetadata, L1BatchNumber,
};

/// Storage key for a [AggregationWrapper`].
#[derive(Debug, Clone, Copy)]
pub struct AggregationsKey {
    pub block_number: L1BatchNumber,
    pub circuit_id: u8,
    pub depth: u16,
}

/// Storage key for a [ClosedFormInputWrapper`].
#[derive(Debug, Clone, Copy)]
pub struct ClosedFormInputKey {
    pub block_number: L1BatchNumber,
    pub circuit_id: u8,
}

/// Storage key for a [`CircuitWrapper`].
#[derive(Debug, Clone, Copy)]
pub struct FriCircuitKey {
    pub block_number: L1BatchNumber,
    pub sequence_number: usize,
    pub circuit_id: u8,
    pub aggregation_round: AggregationRound,
    pub depth: u16,
}

impl From<FriProverJobMetadata> for FriCircuitKey {
    fn from(prover_job_metadata: FriProverJobMetadata) -> Self {
        FriCircuitKey {
            block_number: prover_job_metadata.block_number,
            sequence_number: prover_job_metadata.sequence_number,
            circuit_id: prover_job_metadata.circuit_id,
            aggregation_round: prover_job_metadata.aggregation_round,
            depth: prover_job_metadata.depth,
        }
    }
}

/// Storage key for a [`ZkSyncCircuit`].
#[derive(Debug, Clone, Copy)]
pub struct CircuitKey<'a> {
    pub block_number: L1BatchNumber,
    pub sequence_number: usize,
    pub circuit_type: &'a str,
    pub aggregation_round: AggregationRound,
}

/// Storage key for a [`RamPermutationQueueWitness`].
#[derive(Debug, Clone, Copy)]
pub struct RamPermutationQueueWitnessKey {
    pub block_number: L1BatchNumber,
    pub circuit_subsequence_number: usize,
    pub is_sorted: bool,
}
