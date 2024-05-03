use zksync_types::{
    commitment::L1BatchWithMetadata, ethabi::Token, web3::contract::tokens::Tokenizable,
};

use crate::{i_executor::structures::StoredBatchInfo, Tokenize};

/// Input required to encode `executeBatches` call.
#[derive(Debug, Clone)]
pub struct ExecuteBatches {
    pub l1_batches: Vec<L1BatchWithMetadata>,
}

impl Tokenize for ExecuteBatches {
    fn into_tokens(self) -> Vec<Token> {
        vec![Token::Array(
            self.l1_batches
                .iter()
                .map(|batch| StoredBatchInfo(batch).into_token())
                .collect(),
        )]
    }
}
