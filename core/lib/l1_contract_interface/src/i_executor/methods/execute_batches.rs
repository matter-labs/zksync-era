use zksync_types::{commitment::L1BatchWithMetadata, ethabi::Token};

use crate::{i_executor::structures::StoredBatchInfo, ToEthArgs};

/// Input required to encode `executeBatches` call.
#[derive(Debug, Clone)]
pub struct ExecuteBatches {
    pub l1_batches: Vec<L1BatchWithMetadata>,
}

impl ToEthArgs for ExecuteBatches {
    fn to_eth_args(&self) -> Vec<Token> {
        vec![Token::Array(
            self.l1_batches
                .iter()
                .map(|batch| Token::from(StoredBatchInfo(batch)))
                .collect(),
        )]
    }
}
