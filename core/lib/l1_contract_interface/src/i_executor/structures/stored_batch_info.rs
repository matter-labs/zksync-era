use zksync_types::{
    commitment::L1BatchWithMetadata, ethabi::Token, web3::contract::Error as ContractError, U256,
};

use crate::Tokenizable;

/// Encoding for `StoredBatchInfo` from `IExecutor.sol`
#[derive(Debug)]
pub struct StoredBatchInfo<'a>(pub &'a L1BatchWithMetadata);

impl<'a> Tokenizable for StoredBatchInfo<'a> {
    fn from_token(_token: Token) -> Result<Self, ContractError> {
        // Currently there is no need to decode this struct.
        // We still want to implement `Tokenizable` trait for it, so that *once* it's needed
        // the implementation is provided here and not in some other inconsistent way.
        Err(ContractError::Other("Not implemented".into()))
    }

    fn into_token(self) -> Token {
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
