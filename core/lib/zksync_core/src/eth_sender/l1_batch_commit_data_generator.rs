use zksync_l1_contract_interface::{
    i_executor::{
        methods::{CommitBatchesRollup, CommitBatchesValidium},
        structures::{CommitBatchInfoRollup, CommitBatchInfoValidium},
    },
    Tokenizable, Tokenize,
};
use zksync_types::{commitment::L1BatchWithMetadata, ethabi::Token};

pub trait L1BatchCommitDataGenerator
where
    Self: std::fmt::Debug + Send + Sync,
{
    fn l1_commit_batches(
        &self,
        last_committed_l1_batch: &L1BatchWithMetadata,
        l1_batches: &[L1BatchWithMetadata],
    ) -> Vec<Token>;

    fn l1_commit_batch(&self, l1_batch: &L1BatchWithMetadata) -> Token;
}

#[derive(Debug, Clone)]
pub struct RollupModeL1BatchCommitDataGenerator;

#[derive(Debug, Clone)]
pub struct ValidiumModeL1BatchCommitDataGenerator;

impl L1BatchCommitDataGenerator for RollupModeL1BatchCommitDataGenerator {
    fn l1_commit_batches(
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

    fn l1_commit_batch(&self, l1_batch: &L1BatchWithMetadata) -> Token {
        CommitBatchInfoRollup {
            l1_batch_with_metadata: l1_batch,
        }
        .into_token()
    }
}

impl L1BatchCommitDataGenerator for ValidiumModeL1BatchCommitDataGenerator {
    fn l1_commit_batches(
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

    fn l1_commit_batch(&self, l1_batch: &L1BatchWithMetadata) -> Token {
        CommitBatchInfoValidium {
            l1_batch_with_metadata: l1_batch,
        }
        .into_token()
    }
}
