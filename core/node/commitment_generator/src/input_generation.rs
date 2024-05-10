use std::fmt;

use zksync_types::{commitment::CommitmentInput, H256};

#[derive(Debug)]
pub struct ValidiumInputGenerator;

#[derive(Debug)]
pub struct RollupInputGenerator;

/// Definition of trait handling computing the input needed for batch commitments. At the current moment
/// this is run after we've already pulled the data but in the future this should handle taking raw
/// data and transforming it into the input.
pub trait InputGenerator: 'static + fmt::Debug + Send + Sync {
    fn compute_input(&self, input: CommitmentInput) -> CommitmentInput;
}

impl InputGenerator for ValidiumInputGenerator {
    fn compute_input(&self, input: CommitmentInput) -> CommitmentInput {
        match input {
            CommitmentInput::PostBoojum {
                common,
                system_logs,
                state_diffs,
                aux_commitments,
                blob_commitments,
            } => CommitmentInput::PostBoojum {
                common,
                system_logs,
                state_diffs,
                aux_commitments,
                blob_commitments: vec![H256::zero(); blob_commitments.len()],
            },
            _ => input,
        }
    }
}

impl InputGenerator for RollupInputGenerator {
    fn compute_input(&self, input: CommitmentInput) -> CommitmentInput {
        input
    }
}
