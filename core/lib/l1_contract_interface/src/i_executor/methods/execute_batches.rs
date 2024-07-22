use std::ops::RangeInclusive;
use zksync_types::{commitment::L1BatchWithMetadata, ethabi::Token, L1BatchNumber};

use crate::{i_executor::structures::StoredBatchInfo, Tokenizable, Tokenize};

/// Input required to encode `executeBatches` call.
#[derive(Debug, Clone)]
pub struct ExecuteBatches {
    pub l1_batches: Vec<L1BatchWithMetadata>,
}

impl ExecuteBatches {
    pub fn l1_batch_range(&self) -> RangeInclusive<L1BatchNumber> {
        let batch_num_iter = self
            .l1_batches
            .iter()
            .map(|b| b.header.number)
            .collect::<Vec<_>>();
        batch_num_iter.iter().min().copied().unwrap_or_default()
            ..=batch_num_iter.iter().max().copied().unwrap_or_default()
    }
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
