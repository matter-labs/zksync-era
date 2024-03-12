use std::sync::Arc;

use zkevm_test_harness_1_4_2::kzg::KzgSettings;
use zksync_types::{commitment::L1BatchWithMetadata, ethabi::Token, pubdata_da::PubdataDA};

use crate::{
    i_executor::structures::{CommitBatchInfo, StoredBatchInfo},
    Tokenizable, Tokenize,
};

/// Input required to encode `commitBatches` call.
#[derive(Debug, Clone)]
pub struct CommitBatches {
    pub last_committed_l1_batch: L1BatchWithMetadata,
    pub l1_batches: Vec<L1BatchWithMetadata>,
    pub pubdata_da: PubdataDA,
    pub kzg_settings: Option<Arc<KzgSettings>>,
}

impl Tokenize for CommitBatches {
    fn into_tokens(self) -> Vec<Token> {
        let stored_batch_info = StoredBatchInfo(&self.last_committed_l1_batch).into_token();
        let l1_batches_to_commit = self
            .l1_batches
            .iter()
            .map(|batch| {
                CommitBatchInfo::new(batch, self.pubdata_da, self.kzg_settings.clone()).into_token()
            })
            .collect();

        vec![stored_batch_info, Token::Array(l1_batches_to_commit)]
    }
}
