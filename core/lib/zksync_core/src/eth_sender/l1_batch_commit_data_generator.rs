use zksync_l1_contract_interface::{
    i_executor::methods::{CommitBatchesRollup, CommitBatchesValidium},
    Tokenize,
};
use zksync_types::{commitment::L1BatchWithMetadata, ethabi::Token};

pub trait L1BatchCommitDataGenerator
where
    Self: std::fmt::Debug + Send + Sync,
{
    fn l1_commit_data(
        &self,
        last_committed_l1_batch: &L1BatchWithMetadata,
        l1_batches: &[L1BatchWithMetadata],
    ) -> Vec<Token>;
}

#[derive(Debug, Clone)]
pub struct RollupModeL1BatchCommitDataGenerator;

#[derive(Debug, Clone)]
pub struct ValidiumModeL1BatchCommitDataGenerator;

impl L1BatchCommitDataGenerator for RollupModeL1BatchCommitDataGenerator {
    fn l1_commit_data(
        &self,
        last_committed_l1_batch: &L1BatchWithMetadata,
        l1_batches: &[L1BatchWithMetadata],
    ) -> Vec<Token> {
        CommitBatchesRollup {
            last_committed_l1_batch: last_committed_l1_batch.clone(),
            l1_batches: l1_batches.to_vec(),
        }
        .into_tokens()
    }
}

impl L1BatchCommitDataGenerator for ValidiumModeL1BatchCommitDataGenerator {
    fn l1_commit_data(
        &self,
        last_committed_l1_batch: &L1BatchWithMetadata,
        l1_batches: &[L1BatchWithMetadata],
    ) -> Vec<Token> {
        CommitBatchesValidium {
            last_committed_l1_batch: last_committed_l1_batch.clone(),
            l1_batches: l1_batches.to_vec(),
        }
        .into_tokens()
    }
}
