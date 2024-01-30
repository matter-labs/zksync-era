use zksync_types::{commitment::L1BatchWithMetadata, ethabi::Token};

use crate::{
    i_executor::structures::{CommitBatchInfo, StoredBatchInfo},
    ToEthArgs,
};

/// Input required to encode `commitBatches` call.
#[derive(Debug, Clone)]
pub struct CommitBatches {
    pub last_committed_l1_batch: L1BatchWithMetadata,
    pub l1_batches: Vec<L1BatchWithMetadata>,
}

impl ToEthArgs for CommitBatches {
    fn to_eth_args(&self) -> Vec<Token> {
        let stored_batch_info = Token::from(StoredBatchInfo(&self.last_committed_l1_batch));
        let l1_batches_to_commit = self
            .l1_batches
            .iter()
            .map(|batch| Token::from(CommitBatchInfo(batch)))
            .collect();

        vec![stored_batch_info, Token::Array(l1_batches_to_commit)]
    }
}
