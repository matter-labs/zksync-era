use zksync_types::{
    H256, web3,
    commitment::L1BatchWithMetadata, ethabi::{self,Token}, web3::contract::Error as ContractError, U256,
};

use crate::Tokenizable;

/// Encoding for `StoredBatchInfo` from `IExecutor.sol`
#[derive(Debug)]
pub struct StoredBatchInfo<'a>(pub &'a L1BatchWithMetadata);

/// Compact representation the the `StoredBatchInfo` from `IExecutor.sol`.
#[derive(Debug,Clone)]
pub struct StoredBatchInfoCompact {
    pub batch_number: u64,
    pub batch_hash: H256,
    pub index_repeated_storage_changes: u64,
    pub number_of_layer1_txs: U256,
    pub priority_operations_hash: H256,
    pub l2_logs_tree_root: H256,
    pub timestamp: U256,
    pub commitment: H256,
}

impl StoredBatchInfoCompact {
    /// `_hashStoredBatchInfo` from `Executor.sol`.
    pub fn hash(&self) -> H256 {
        H256(web3::keccak256(&ethabi::encode(&[self.clone().into_token()])))
    }
}

impl<'a> StoredBatchInfo<'a> {
    pub fn into_compact(self) -> StoredBatchInfoCompact {
        StoredBatchInfoCompact {
            batch_number: self.0.header.number.0.into(),
            batch_hash: self.0.metadata.root_hash,
            index_repeated_storage_changes: self.0.metadata.rollup_last_leaf_index,
            number_of_layer1_txs: self.0.header.l1_tx_count.into(),
            priority_operations_hash: self.0
                .header
                .priority_ops_onchain_data_hash(),
            l2_logs_tree_root: self.0.metadata.l2_l1_merkle_root,
            timestamp: self.0.header.timestamp.into(),
            commitment: self.0.metadata.commitment,
        }
    }
}

impl<'a> Tokenizable for StoredBatchInfo<'a> {
    fn from_token(_token: Token) -> Result<Self, ContractError> {
        // Currently there is no need to decode this struct.
        // We still want to implement `Tokenizable` trait for it, so that *once* it's needed
        // the implementation is provided here and not in some other inconsistent way.
        Err(ContractError::Other("Not implemented".into()))
    }

    fn into_token(self) -> Token {
        self.into_compact().into_token()
    }
}

impl<'a> Tokenizable for StoredBatchInfoCompact {
    fn from_token(_token: Token) -> Result<Self, ContractError> {
        // Currently there is no need to decode this struct.
        // We still want to implement `Tokenizable` trait for it, so that *once* it's needed
        // the implementation is provided here and not in some other inconsistent way.
        Err(ContractError::Other("Not implemented".into()))
    }

    fn into_token(self) -> Token {
        Token::Tuple(vec![
            Token::Uint(self.batch_number.into()),
            Token::FixedBytes(self.batch_hash.as_bytes().to_vec()),
            Token::Uint(self.index_repeated_storage_changes.into()),
            Token::Uint(self.number_of_layer1_txs),
            Token::FixedBytes(self.priority_operations_hash.as_bytes().to_vec()),
            Token::FixedBytes(self.l2_logs_tree_root.as_bytes().to_vec()),
            Token::Uint(self.timestamp),
            Token::FixedBytes(self.commitment.as_bytes().to_vec()),
        ])
    }
}
