use std::fmt;

use zksync_types::{
    commitment::{L1BatchAuxiliaryOutput, L1BatchCommitment},
    H256,
};

#[derive(Debug)]
pub struct ValidiumCommitmentPostProcessor;

#[derive(Debug)]
pub struct RollupCommitmentPostProcessor;

/// Definition of trait handling post processing the `L1BatchCommitment` depending on the DA solution
/// being utilized.
pub trait CommitmentPostProcessor: 'static + fmt::Debug + Send + Sync {
    fn post_process_commitment(&self, commitment: L1BatchCommitment) -> L1BatchCommitment;
}

impl CommitmentPostProcessor for ValidiumCommitmentPostProcessor {
    fn post_process_commitment(&self, mut commitment: L1BatchCommitment) -> L1BatchCommitment {
        let aux_output = match commitment.aux_output() {
            L1BatchAuxiliaryOutput::PostBoojum {
                common,
                system_logs_linear_hash,
                state_diffs_compressed,
                state_diffs_hash,
                aux_commitments,
                blob_linear_hashes,
                blob_commitments,
            } => L1BatchAuxiliaryOutput::PostBoojum {
                common,
                system_logs_linear_hash,
                state_diffs_compressed,
                state_diffs_hash,
                aux_commitments,
                blob_linear_hashes: vec![H256::zero(); blob_linear_hashes.len()],
                blob_commitments: vec![H256::zero(); blob_commitments.len()],
            },
            _ => commitment.aux_output(),
        };

        commitment.auxiliary_output = aux_output;
        commitment
    }
}

impl CommitmentPostProcessor for RollupCommitmentPostProcessor {
    fn post_process_commitment(&self, commitment: L1BatchCommitment) -> L1BatchCommitment {
        commitment
    }
}
