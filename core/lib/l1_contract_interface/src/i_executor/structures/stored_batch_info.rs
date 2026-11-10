use anyhow::Context as _;
use zksync_types::{
    commitment::L1BatchWithMetadata,
    ethabi::{self, ParamType, Token},
    parse_h256, web3, ProtocolVersionId, H256, U256,
};

pub const MESSAGE_ROOT_ROLLING_HASH_KEY: H256 = H256([
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07,
]);

/// `StoredBatchInfo` from `IExecutor.sol`.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredBatchInfo {
    pub batch_number: u64,
    pub batch_hash: H256,
    pub index_repeated_storage_changes: u64,
    pub number_of_layer1_txs: U256,
    pub priority_operations_hash: H256,
    pub dependency_roots_rolling_hash: H256,
    pub l2_logs_tree_root: H256,
    pub timestamp: U256,
    pub commitment: H256,
    pub airbender_commitment: H256,
}

impl StoredBatchInfo {
    pub fn schema_for_protocol_version(protocol_version: ProtocolVersionId) -> ParamType {
        if protocol_version >= ProtocolVersionId::Version34 {
            let ParamType::Tuple(mut fields) = Self::schema_post_interop() else {
                unreachable!()
            };
            fields.push(ParamType::FixedBytes(32));
            ParamType::Tuple(fields)
        } else if protocol_version.is_pre_interop_fast_blocks() {
            Self::schema_pre_interop()
        } else {
            Self::schema_post_interop()
        }
    }

    pub fn schema_pre_interop() -> ParamType {
        ParamType::Tuple(vec![
            ParamType::Uint(64),       // `batch_number`
            ParamType::FixedBytes(32), // `batch_hash`
            ParamType::Uint(64),       // `index_repeated_storage_changes`
            ParamType::Uint(256),      // `number_of_layer1_txs`
            ParamType::FixedBytes(32), // `priority_operations_hash`
            ParamType::FixedBytes(32), // `l2_logs_tree_root`
            ParamType::Uint(256),      // `timestamp`
            ParamType::FixedBytes(32), // `commitment`
        ])
    }

    pub fn schema_post_interop() -> ParamType {
        ParamType::Tuple(vec![
            ParamType::Uint(64),       // `batch_number`
            ParamType::FixedBytes(32), // `batch_hash`
            ParamType::Uint(64),       // `index_repeated_storage_changes`
            ParamType::Uint(256),      // `number_of_layer1_txs`
            ParamType::FixedBytes(32), // `priority_operations_hash`
            ParamType::FixedBytes(32), // `dependency_roots_rolling_hash`
            ParamType::FixedBytes(32), // `l2_logs_tree_root`
            ParamType::Uint(256),      // `timestamp`
            ParamType::FixedBytes(32), // `commitment`
        ])
    }

    /// Encodes the contract struct for the given protocol version.
    pub fn encode(&self, protocol_version: ProtocolVersionId) -> Vec<u8> {
        ethabi::encode(&[self
            .clone()
            .into_token_with_protocol_version(protocol_version)])
    }

    /// Decodes the contract struct for the given protocol version.
    pub fn decode(rlp: &[u8], protocol_version: ProtocolVersionId) -> anyhow::Result<Self> {
        let [token] =
            ethabi::decode_whole(&[Self::schema_for_protocol_version(protocol_version)], rlp)?
                .try_into()
                .unwrap();
        Self::from_token(token)
    }

    /// `_hashStoredBatchInfo` from `Executor.sol`.
    pub fn hash(&self, protocol_version: ProtocolVersionId) -> H256 {
        H256(web3::keccak256(&self.encode(protocol_version)))
    }

    pub fn from_token(token: Token) -> anyhow::Result<Self> {
        let mut tokens = token.into_tuple().context("not a tuple")?;
        let airbender_commitment = if tokens.len() == 10 {
            let Token::FixedBytes(value) = tokens.pop().unwrap() else {
                anyhow::bail!("bad Airbender commitment")
            };
            parse_h256(&value).context("airbender_commitment")?
        } else {
            H256::zero()
        };
        if tokens.len() == 8 {
            tokens.insert(5, Token::FixedBytes(vec![0; 32]));
        }
        let [
            Token::Uint(batch_number),
            Token::FixedBytes(batch_hash),
            Token::Uint(index_repeated_storage_changes),
            Token::Uint(number_of_layer1_txs),
            Token::FixedBytes(priority_operations_hash),
            Token::FixedBytes(dependency_roots_rolling_hash),
            Token::FixedBytes(l2_logs_tree_root),
            Token::Uint(timestamp),
            Token::FixedBytes(commitment),
        ] : [Token;9] = tokens
            .try_into().ok().context("bad length")?
        else { anyhow::bail!("bad format") };
        Ok(Self {
            airbender_commitment,
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
            dependency_roots_rolling_hash: parse_h256(&dependency_roots_rolling_hash)
                .context("dependency_roots_rolling_hash")?,
            l2_logs_tree_root: parse_h256(&l2_logs_tree_root).context("l2_logs_tree_root")?,
            timestamp,
            commitment: parse_h256(&commitment).context("commitment")?,
        })
    }

    pub fn into_token_with_protocol_version(self, protocol_version: ProtocolVersionId) -> Token {
        if protocol_version.is_pre_interop_fast_blocks() {
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
        } else {
            let mut tokens = vec![
                Token::Uint(self.batch_number.into()),
                Token::FixedBytes(self.batch_hash.as_bytes().to_vec()),
                Token::Uint(self.index_repeated_storage_changes.into()),
                Token::Uint(self.number_of_layer1_txs),
                Token::FixedBytes(self.priority_operations_hash.as_bytes().to_vec()),
                Token::FixedBytes(self.dependency_roots_rolling_hash.as_bytes().to_vec()),
                Token::FixedBytes(self.l2_logs_tree_root.as_bytes().to_vec()),
                Token::Uint(self.timestamp),
                Token::FixedBytes(self.commitment.as_bytes().to_vec()),
            ];
            if protocol_version >= ProtocolVersionId::Version34 {
                tokens.push(Token::FixedBytes(
                    self.airbender_commitment.as_bytes().to_vec(),
                ));
            }
            Token::Tuple(tokens)
        }
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
            dependency_roots_rolling_hash: if x
                .header
                .protocol_version
                .unwrap_or(ProtocolVersionId::Version0)
                .is_pre_interop_fast_blocks()
            {
                H256::zero()
            } else {
                x.header
                    .system_logs
                    .iter()
                    .find(|log| log.0.key == MESSAGE_ROOT_ROLLING_HASH_KEY)
                    .map(|log| log.0.value)
                    .unwrap_or(H256::zero())
            },
            l2_logs_tree_root: x.metadata.l2_l1_merkle_root,
            timestamp: x.header.timestamp.into(),
            commitment: x.metadata.commitment,
            airbender_commitment: if x.header.protocol_version >= Some(ProtocolVersionId::Version34)
            {
                x.metadata
                    .airbender
                    .as_ref()
                    .expect("missing Airbender commitment")
                    .commitment
            } else {
                H256::zero()
            },
        }
    }
}
