use zksync_types::{commitment::L1BatchWithMetadata, ethabi::Token};

use crate::{i_executor::structures::StoredBatchInfo, Tokenizable, Tokenize};

/// Input required to encode `executeBatches` call.
#[derive(Debug, Clone)]
pub struct ExecuteBatches {
    pub l1_batches: Vec<L1BatchWithMetadata>,
}

impl Tokenize for &ExecuteBatches {
    fn into_tokens(self) -> Vec<Token> {
        vec![Token::Array(
            self.l1_batches
                .iter()
                .map(|batch| StoredBatchInfo::from(batch).into_token())
                .collect(),
        )]
    }
}
