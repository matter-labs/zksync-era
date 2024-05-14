use zksync_types::{commitment::L1BatchWithMetadata, ethabi::Token, pubdata_da::PubdataDA};

use crate::{
    i_executor::structures::{CommitBatchInfoRollup, CommitBatchInfoValidium, StoredBatchInfo},
    Tokenizable, Tokenize,
};

/// Input required to encode `commitBatches` call for a contract running in rollup mode.
#[derive(Debug, Clone)]
pub struct CommitBatchesRollup {
    pub last_committed_l1_batch: L1BatchWithMetadata,
    pub l1_batches: Vec<L1BatchWithMetadata>,
    pub pubdata_da: PubdataDA,
}

impl Tokenize for CommitBatchesRollup {
    fn into_tokens(self) -> Vec<Token> {
        let stored_batch_info = StoredBatchInfo(&self.last_committed_l1_batch).into_token();
        let l1_batches_to_commit = self
            .l1_batches
            .iter()
            .map(|batch| CommitBatchInfoRollup::new(batch, self.pubdata_da).into_token())
            .collect();

        vec![stored_batch_info, Token::Array(l1_batches_to_commit)]
    }
}

/// Input required to encode `commitBatches` call for a contract running in validium mode.
#[derive(Debug, Clone)]
pub struct CommitBatchesValidium {
    pub last_committed_l1_batch: L1BatchWithMetadata,
    pub l1_batches: Vec<L1BatchWithMetadata>,
    pub pubdata_da: PubdataDA,
}

impl Tokenize for CommitBatchesValidium {
    fn into_tokens(self) -> Vec<Token> {
        let stored_batch_info = StoredBatchInfo(&self.last_committed_l1_batch).into_token();
        let l1_batches_to_commit = self
            .l1_batches
            .iter()
            .map(|batch| CommitBatchInfoValidium::new(batch, self.pubdata_da).into_token())
            .collect();

        vec![stored_batch_info, Token::Array(l1_batches_to_commit)]
    }
}
