use zksync_l1_contract_interface::{
    i_executor::{
        methods::{CommitBatchesRollup, CommitBatchesValidium},
        structures::{CommitBatchInfoRollup, CommitBatchInfoValidium},
    },
    Tokenizable, Tokenize,
};
use zksync_types::{commitment::L1BatchWithMetadata, ethabi::Token};

/// [`L1BatchCommitDataGenerator`] abstracts how a batch or list of batches need to be tokenized
/// for committing to L1.
pub trait L1BatchCommitDataGenerator
where
    Self: std::fmt::Debug + Send + Sync,
{
    /// [`l1_commit_batches`] is the method you want when committing a block.
    /// It uses metadata from `last_committed_l1_batch` and the batches in `l1_batches` to produce
    /// the full commit data.
    fn l1_commit_batches(
        &self,
        last_committed_l1_batch: &L1BatchWithMetadata,
        l1_batches: &[L1BatchWithMetadata],
    ) -> Vec<Token>;

    /// [`l1_commit_batch`] is used mostly for size calculations for sealing criteria and for
    /// consistency checks. Instead of preparing a full commit, it will tokenize an individual batch.
    fn l1_commit_batch(&self, l1_batch: &L1BatchWithMetadata) -> Token;
}

/// [`RollupModeL1BatchCommitDataGenerator`] implements [`L1BatchCommitDataGenerator`] for
/// contracts operating in rollup mode. It differs from [`ValidiumModeL1BatchCommitDataGenerator`]
/// in that it includes the pubdata in the produced message.
#[derive(Debug, Clone)]
pub struct RollupModeL1BatchCommitDataGenerator;

/// [`ValidiumModeL1BatchCommitDataGenerator`] implements [`L1BatchCommitDataGenerator`] for
/// contracts operating in validium mode. It differs from [`RollupModeL1BatchCommitDataGenerator`]
/// in that it does not include the pubdata in the produced message.
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
