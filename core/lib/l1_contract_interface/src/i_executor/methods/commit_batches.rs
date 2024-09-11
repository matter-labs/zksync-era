use zksync_types::{
    commitment::{L1BatchCommitmentMode, L1BatchWithMetadata},
    ethabi::Token,
    pubdata_da::PubdataDA,
};

use crate::{
    i_executor::structures::{CommitBatchInfo, StoredBatchInfo},
    Tokenizable, Tokenize,
};

/// Input required to encode `commitBatches` call for a contract
#[derive(Debug)]
pub struct CommitBatches<'a> {
    pub last_committed_l1_batch: &'a L1BatchWithMetadata,
    pub l1_batches: &'a [L1BatchWithMetadata],
    pub pubdata_da: PubdataDA,
    pub mode: L1BatchCommitmentMode,
}

impl Tokenize for CommitBatches<'_> {
    fn into_tokens(self) -> Vec<Token> {
        let stored_batch_info = StoredBatchInfo::from(self.last_committed_l1_batch).into_token();
        let l1_batches_to_commit = self
            .l1_batches
            .iter()
            .map(|batch| CommitBatchInfo::new(self.mode, batch, self.pubdata_da).into_token())
            .collect();

        vec![stored_batch_info, Token::Array(l1_batches_to_commit)]
    }
}
