use zksync_types::{commitment::L1BatchWithMetadata, ethabi::Token, U256};

use crate::IntoTokens;

/// Encoding for `StoredBatchInfo` from `IExecutor.sol`
#[derive(Debug)]
pub struct StoredBatchInfo<'a>(pub &'a L1BatchWithMetadata);

impl<'a> IntoTokens for StoredBatchInfo<'a> {
    fn into_tokens(self) -> Token {
        Token::Tuple(vec![
            // `batchNumber`
            Token::Uint(U256::from(self.0.header.number.0)),
            // `batchHash`
            Token::FixedBytes(self.0.metadata.root_hash.as_bytes().to_vec()),
            // `indexRepeatedStorageChanges`
            Token::Uint(U256::from(self.0.metadata.rollup_last_leaf_index)),
            // `numberOfLayer1Txs`
            Token::Uint(U256::from(self.0.header.l1_tx_count)),
            // `priorityOperationsHash`
            Token::FixedBytes(
                self.0
                    .header
                    .priority_ops_onchain_data_hash()
                    .as_bytes()
                    .to_vec(),
            ),
            // `l2LogsTreeRoot`
            Token::FixedBytes(self.0.metadata.l2_l1_merkle_root.as_bytes().to_vec()),
            // timestamp
            Token::Uint(U256::from(self.0.header.timestamp)),
            // commitment
            Token::FixedBytes(self.0.metadata.commitment.as_bytes().to_vec()),
        ])
    }
}
