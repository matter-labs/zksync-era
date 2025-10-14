use std::{collections::HashMap, str::FromStr, sync::Arc};

use ethabi;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zksync_basic_types::{u256_to_h256, web3::keccak256, Address, H160, H256, U256};
use zksync_types::{priority_op_onchain_data::PriorityOpOnchainData, L2_TO_L1_LOGS_TREE_ROOT_KEY};

use self::{v1::V1, v2::V2, v3::V3};
use crate::{
    l1_fetcher::{
        blob_http_client::BlobClient,
        constants::zksync::L2_TO_L1_LOG_SERIALIZE_SIZE,
        types::common::{parse_l2_to_l1_log, read_next_n_bytes},
    },
    storage::PackingType,
};

// NOTE: We should probably make these more human-readable.
pub mod common;
pub mod v1;
pub mod v2;
pub mod v3;

#[allow(clippy::enum_variant_names)]
#[derive(Error, Debug, Clone)]
pub enum ParseError {
    #[error("invalid Calldata: {0}")]
    InvalidCalldata(String),

    #[error("invalid StoredBlockInfo: {0}")]
    InvalidStoredBlockInfo(String),

    #[error("invalid CommitBlockInfo: {0}")]
    InvalidCommitBlockInfo(String),

    #[allow(dead_code)]
    #[error("invalid compressed bytecode: {0}")]
    InvalidCompressedByteCode(String),

    #[error("invalid compressed value: {0}")]
    InvalidCompressedValue(String),

    #[error("invalid pubdata source: {0}")]
    InvalidPubdataSource(String),

    #[error("blob storage error: {0}")]
    BlobStorageError(String),

    #[error("blob format error: {0}")]
    BlobFormatError(String, String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum L2ToL1Pubdata {
    L2ToL1Log {
        shard_id: u8,
        is_service: bool,
        tx_number_in_block: u16,
        sender: Address,
        key: H256,
        value: H256,
    },
    L2ToL2Message(Vec<u8>),
    PublishedBytecode(Vec<u8>),
    CompressedStateDiff {
        is_repeated_write: bool,
        derived_key: U256,
        packing_type: PackingType,
    },
}

pub trait CommitBlockFormat {
    fn to_enum_variant(self) -> CommitBlockInfo;
}

#[derive(Debug)]
pub enum CommitBlockInfo {
    V1(V1),
    V2(V2),
}

/// Block with all required fields extracted from a [`CommitBlockInfo`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitBlock {
    /// ZKSync batch number.
    pub l1_batch_number: u64,
    /// Next unused key serial number.
    pub index_repeated_storage_changes: u64,
    /// The state root of the full state tree.
    pub new_state_root: Vec<u8>,
    /// Storage write access as a concatenation key-value.
    pub initial_storage_changes: IndexMap<U256, PackingType>,
    /// Storage write access as a concatenation index-value.
    pub repeated_storage_changes: IndexMap<u64, PackingType>,
    /// (contract bytecodes) array of L2 bytecodes that were deployed.
    pub factory_deps: Vec<Vec<u8>>,

    pub l1_tx_count: u64,

    pub timestamp: u64,
    pub priority_operations_hash: H256,
    pub priority_ops_onchain_data: Vec<PriorityOpOnchainData>,
    pub l2_logs_tree_root: H256,
    pub commitment: H256,
    pub rollup_last_leaf_index: u64,
}

impl CommitBlock {
    pub fn try_from_token<'a, F>(value: &'a ethabi::Token) -> Result<Self, ParseError>
    where
        F: CommitBlockFormat + TryFrom<&'a ethabi::Token, Error = ParseError>,
    {
        let commit_block_info = F::try_from(value)?;
        Ok(Self::from_commit_block(commit_block_info.to_enum_variant()))
    }

    pub async fn try_from_token_resolve(
        value: &ethabi::Token,
        client: &Arc<dyn BlobClient>,
    ) -> Result<Self, ParseError> {
        let commit_block_info = V3::try_from(value)?;
        Self::from_commit_block_resolve(commit_block_info, client).await
    }

    pub fn uncompress_bytecode(compressed_bytes: &[u8]) -> Vec<u8> {
        // decompression based on publishCompressedBytecode from Compressor.sol
        let mut ptr = 0;
        let dictionary_len: usize =
            8 * u16::from_be_bytes(read_next_n_bytes(compressed_bytes, &mut ptr)) as usize;
        let dictionary_bytes = compressed_bytes[ptr..ptr + dictionary_len].to_vec();
        ptr += dictionary_len;

        let mut result: Vec<u8> = vec![];
        let encoded_data_len = (compressed_bytes.len() - ptr) / 2;
        for _ in 0..encoded_data_len {
            let mut index_of_encoded_chunk: usize =
                8 * u16::from_be_bytes(read_next_n_bytes(compressed_bytes, &mut ptr)) as usize;
            let mut decoded =
                read_next_n_bytes::<8>(&dictionary_bytes, &mut index_of_encoded_chunk).to_vec();
            result.append(&mut decoded);
        }
        result
    }

    fn factory_deps_from_pubdata(pubdata: &Vec<L2ToL1Pubdata>) -> Vec<Vec<u8>> {
        let mut factory_deps = Vec::new();
        let mut hashed_contracts_messages = Vec::new();
        let mut l1_messages: HashMap<H256, Vec<u8>> = HashMap::new();
        for log in pubdata {
            match log {
                L2ToL1Pubdata::L2ToL1Log {
                    sender, key, value, ..
                } => {
                    if sender
                        == &H160::from_str("0x0000000000000000000000000000000000008008").unwrap()
                        && key == &H256::from_str(
                            "0x000000000000000000000000000000000000000000000000000000000000800e",
                        )
                        .unwrap()
                    {
                        hashed_contracts_messages.push(value);
                    }
                }
                L2ToL1Pubdata::L2ToL2Message(bytes) => {
                    l1_messages.insert(H256::from(keccak256(bytes)), bytes.clone());
                }
                _ => (),
            }
        }
        for hashed_contracts_message in hashed_contracts_messages {
            if let Some(l2_message) = l1_messages.get(hashed_contracts_message) {
                factory_deps.push(CommitBlock::uncompress_bytecode(l2_message));
            }
        }
        factory_deps
    }

    pub fn from_commit_block(block_type: CommitBlockInfo) -> Self {
        match block_type {
            CommitBlockInfo::V1(block) => CommitBlock {
                l1_batch_number: block.l1_batch_number,
                index_repeated_storage_changes: block.index_repeated_storage_changes,
                new_state_root: block.new_state_root,
                initial_storage_changes: block
                    .initial_storage_changes
                    .into_iter()
                    .map(|(k, v)| (k, PackingType::NoCompression(v.into())))
                    .collect(),
                repeated_storage_changes: block
                    .repeated_storage_changes
                    .into_iter()
                    .map(|(k, v)| (k, PackingType::NoCompression(v.into())))
                    .collect(),
                factory_deps: block.factory_deps,
                l1_tx_count: block.number_of_l1_txs.as_u64(),
                //FIXME
                timestamp: 0,
                priority_operations_hash: Default::default(),
                priority_ops_onchain_data: vec![],
                l2_logs_tree_root: Default::default(),
                commitment: Default::default(),
                rollup_last_leaf_index: 0,
            },
            CommitBlockInfo::V2(block) => {
                let mut initial_storage_changes = IndexMap::new();
                let mut repeated_storage_changes = IndexMap::new();
                let mut factory_deps =
                    CommitBlock::factory_deps_from_pubdata(&block.total_l2_to_l1_pubdata);
                for log in block.total_l2_to_l1_pubdata {
                    match log {
                        L2ToL1Pubdata::PublishedBytecode(bytecode) => factory_deps.push(bytecode),
                        L2ToL1Pubdata::CompressedStateDiff {
                            is_repeated_write,
                            derived_key,
                            packing_type,
                        } => {
                            if is_repeated_write {
                                repeated_storage_changes.insert(derived_key.as_u64(), packing_type);
                            } else {
                                initial_storage_changes.insert(derived_key, packing_type);
                            };
                        }
                        _ => (),
                    }
                }

                CommitBlock {
                    l1_batch_number: block.l1_batch_number,
                    index_repeated_storage_changes: block.index_repeated_storage_changes,
                    new_state_root: block.new_state_root,
                    initial_storage_changes,
                    repeated_storage_changes,
                    factory_deps,
                    l1_tx_count: block.number_of_l1_txs.as_u64(),
                    //FIXME
                    timestamp: 0,
                    priority_operations_hash: Default::default(),
                    priority_ops_onchain_data: vec![],
                    l2_logs_tree_root: Default::default(),
                    commitment: Default::default(),
                    rollup_last_leaf_index: 0,
                }
            }
        }
    }

    fn extract_l2_logs_tree_root(block: &V3) -> H256 {
        let mut pointer = 0;
        let num_of_l1_to_l2_logs = block.system_logs.len() / L2_TO_L1_LOG_SERIALIZE_SIZE;
        for _ in 0..num_of_l1_to_l2_logs {
            let log = parse_l2_to_l1_log(&block.system_logs, &mut pointer).unwrap();
            match log {
                L2ToL1Pubdata::L2ToL1Log { key, value, .. } => {
                    if key == u256_to_h256(L2_TO_L1_LOGS_TREE_ROOT_KEY.into()) {
                        return value;
                    }
                }
                _ => panic!("Expected L2ToL1Log"),
            }
        }
        unreachable!("L2 logs tree root not found");
    }
    pub async fn from_commit_block_resolve(
        block: V3,
        client: &Arc<dyn BlobClient>,
    ) -> Result<Self, ParseError> {
        let total_l2_to_l1_pubdata = block.parse_pubdata(client).await?;
        let mut initial_storage_changes = IndexMap::new();
        let mut repeated_storage_changes = IndexMap::new();
        let mut factory_deps = CommitBlock::factory_deps_from_pubdata(&total_l2_to_l1_pubdata);
        for log in total_l2_to_l1_pubdata {
            match log {
                L2ToL1Pubdata::PublishedBytecode(bytecode) => factory_deps.push(bytecode),
                L2ToL1Pubdata::CompressedStateDiff {
                    is_repeated_write,
                    derived_key,
                    packing_type,
                } => {
                    if is_repeated_write {
                        repeated_storage_changes.insert(derived_key.as_u64(), packing_type);
                    } else {
                        initial_storage_changes.insert(derived_key, packing_type);
                    };
                }
                _ => (),
            }
        }

        let l2_logs_tree_root = CommitBlock::extract_l2_logs_tree_root(&block);
        Ok(CommitBlock {
            l1_batch_number: block.l1_batch_number,
            index_repeated_storage_changes: block.index_repeated_storage_changes,
            new_state_root: block.new_state_root,
            initial_storage_changes,
            repeated_storage_changes,
            factory_deps,
            l1_tx_count: block.number_of_l1_txs.as_u64(),
            timestamp: block.timestamp,
            priority_operations_hash: H256::from_slice(&block.priority_operations_hash),
            rollup_last_leaf_index: block.index_repeated_storage_changes,
            l2_logs_tree_root,
            commitment: Default::default(),
            priority_ops_onchain_data: vec![],
        })
    }
}
