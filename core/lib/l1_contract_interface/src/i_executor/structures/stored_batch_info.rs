use anyhow::Context as _;
use zksync_types::{
    commitment::L1BatchWithMetadata,
    ethabi::{self, ParamType, Token},
    parse_h256, web3,
    web3::contract::Error as ContractError,
    H256, U256,
};

use crate::Tokenizable;

/// `StoredBatchInfo` from `IExecutor.sol`.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredBatchInfo {
    pub batch_number: u64,
    pub batch_hash: H256,
    pub index_repeated_storage_changes: u64,
    pub number_of_layer1_txs: U256,
    pub priority_operations_hash: H256,
    pub l2_logs_tree_root: H256,
    pub timestamp: U256,
    pub commitment: H256,
}

impl StoredBatchInfo {
    fn schema() -> Vec<ParamType> {
        vec![ParamType::Tuple(vec![
            ParamType::Uint(64),
            ParamType::FixedBytes(32),
            ParamType::Uint(64),
            ParamType::Uint(256),
            ParamType::FixedBytes(32),
            ParamType::FixedBytes(32),
            ParamType::Uint(256),
            ParamType::FixedBytes(32),
        ])]
    }

    /// Encodes the struct into RLP.
    pub fn encode(&self) -> Vec<u8> {
        ethabi::encode(&[self.clone().into_token()])
    }

    /// Decodes the struct from RLP.
    pub fn decode(rlp: &[u8]) -> anyhow::Result<Self> {
        let [token] = ethabi::decode_whole(&Self::schema(), rlp)?
            .try_into()
            .unwrap();
        Ok(Self::from_token(token)?)
    }

    /// `_hashStoredBatchInfo` from `Executor.sol`.
    pub fn hash(&self) -> H256 {
        H256(web3::keccak256(&self.encode()))
    }
}

impl From<&L1BatchWithMetadata> for StoredBatchInfo {
    fn from(x: &L1BatchWithMetadata) -> Self {
        Self {
            batch_number: x.header.number.0.into(),
            batch_hash: x.metadata.root_hash,
            index_repeated_storage_changes: x.metadata.rollup_last_leaf_index,
            number_of_layer1_txs: x.header.l1_tx_count.into(),
            priority_operations_hash: x.header.priority_ops_onchain_data_hash(),
            l2_logs_tree_root: x.metadata.l2_l1_merkle_root,
            timestamp: x.header.timestamp.into(),
            commitment: x.metadata.commitment,
        }
    }
}

impl Tokenizable for StoredBatchInfo {
    fn from_token(token: Token) -> Result<Self, ContractError> {
        (|| {
            let [
                Token::Uint(batch_number),
                Token::FixedBytes(batch_hash),
                Token::Uint(index_repeated_storage_changes),
                Token::Uint(number_of_layer1_txs),
                Token::FixedBytes(priority_operations_hash),
                Token::FixedBytes(l2_logs_tree_root),
                Token::Uint(timestamp),
                Token::FixedBytes(commitment),
            ] : [Token;8] = token
                .into_tuple().context("not a tuple")?
                .try_into().ok().context("bad length")?
            else { anyhow::bail!("bad format") };
            Ok(Self {
                batch_number: batch_number
                    .try_into()
                    .ok()
                    .context("overflow")
                    .context("batch_number")?,
                batch_hash: parse_h256(&batch_hash).context("batch_hash")?,
                index_repeated_storage_changes: index_repeated_storage_changes
                    .try_into()
                    .ok()
                    .context("overflow")
                    .context("index_repeated_storage_changes")?,
                number_of_layer1_txs,
                priority_operations_hash: parse_h256(&priority_operations_hash)
                    .context("priority_operations_hash")?,
                l2_logs_tree_root: parse_h256(&l2_logs_tree_root).context("l2_logs_tree_root")?,
                timestamp,
                commitment: parse_h256(&commitment).context("commitment")?,
            })
        })()
        .map_err(|err| ContractError::InvalidOutputType(format!("{err:#}")))
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
