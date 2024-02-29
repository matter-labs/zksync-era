use zksync_types::{commitment::L1BatchWithMetadata, ethabi::Token};

use crate::{
    i_executor::structures::{CommitBatchInfoRollup, CommitBatchInfoValidium, StoredBatchInfo},
    Tokenizable, Tokenize,
};

/// Input required to encode `commitBatches` call.
#[derive(Debug, Clone)]
pub struct CommitBatchesRollup {
    pub last_committed_l1_batch: L1BatchWithMetadata,
    pub l1_batches: Vec<L1BatchWithMetadata>,
}

impl Tokenize for CommitBatchesRollup {
    fn into_tokens(self) -> Vec<Token> {
        let stored_batch_info = StoredBatchInfo(&self.last_committed_l1_batch).into_token();
        let l1_batches_to_commit = self
            .l1_batches
            .iter()
            .map(|batch| CommitBatchInfoRollup::new(batch).into_token())
            .collect();

        vec![stored_batch_info, Token::Array(l1_batches_to_commit)]
    }
}

/// Input required to encode `commitBatches` call.
#[derive(Debug, Clone)]
pub struct CommitBatchesValidium {
    pub last_committed_l1_batch: L1BatchWithMetadata,
    pub l1_batches: Vec<L1BatchWithMetadata>,
}

impl Tokenize for CommitBatchesValidium {
    // FIXME: use Validium
    fn into_tokens(self) -> Vec<Token> {
        let stored_batch_info = StoredBatchInfo(&self.last_committed_l1_batch).into_token();
        let l1_batches_to_commit = self
            .l1_batches
            .iter()
            .map(|batch| CommitBatchInfoValidium::new(batch).into_token())
            .collect();

        vec![stored_batch_info, Token::Array(l1_batches_to_commit)]
    }
}
