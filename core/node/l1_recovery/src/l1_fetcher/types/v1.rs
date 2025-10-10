use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use zksync_basic_types::U256;

use super::{CommitBlockFormat, CommitBlockInfo, ParseError};

/// Data needed to commit new block
#[derive(Debug, Serialize, Deserialize)]
pub struct V1 {
    /// ZKSync batch number.
    pub l1_batch_number: u64,
    /// Unix timestamp denoting the start of the block execution.
    pub timestamp: u64,
    /// The serial number of the shortcut index that's used as a unique identifier for storage keys that were used twice or more.
    pub index_repeated_storage_changes: u64,
    /// The state root of the full state tree.
    pub new_state_root: Vec<u8>,
    /// Number of priority operations to be processed.
    pub number_of_l1_txs: U256,
    /// The root hash of the tree that contains all L2 -> L1 logs in the block.
    pub l2_logs_tree_root: Vec<u8>,
    /// Hash of all priority operations from this block.
    pub priority_operations_hash: Vec<u8>,
    /// Storage write access as a concatenation key-value.
    pub initial_storage_changes: IndexMap<U256, [u8; 32]>,
    /// Storage write access as a concatenation index-value.
    pub repeated_storage_changes: IndexMap<u64, [u8; 32]>,
    /// Concatenation of all L2 -> L1 logs in the block.
    pub l2_logs: Vec<u8>,
    /// (contract bytecodes) array of L2 bytecodes that were deployed.
    pub factory_deps: Vec<Vec<u8>>,
}

impl CommitBlockFormat for V1 {
    fn to_enum_variant(self) -> CommitBlockInfo {
        CommitBlockInfo::V1(self)
    }
}

impl TryFrom<&ethabi::Token> for V1 {
    type Error = ParseError;

    /// Try to parse Ethereum ABI token into [`V1`].
    fn try_from(token: &ethabi::Token) -> Result<Self, Self::Error> {
        let ExtractedToken {
            l1_batch_number,
            timestamp,
            new_enumeration_index,
            state_root,
            number_of_l1_txs,
            l2_logs_tree_root,
            priority_operations_hash,
            initial_changes_calldata,
            repeated_changes_calldata,
            l2_logs,
            factory_deps,
        } = token.try_into()?;
        let new_enumeration_index = new_enumeration_index.0[0];

        let mut smartcontracts = vec![];
        for bytecode in &factory_deps {
            let ethabi::Token::Bytes(bytecode) = bytecode else {
                return Err(ParseError::InvalidCommitBlockInfo(
                    "factoryDeps".to_string(),
                ));
            };

            smartcontracts.push(bytecode.clone());
        }

        assert_eq!(repeated_changes_calldata.len() % 40, 4);

        tracing::trace!(
            "Have {} new keys",
            (initial_changes_calldata.len() - 4) / 64
        );
        tracing::trace!(
            "Have {} repeated keys",
            (repeated_changes_calldata.len() - 4) / 40
        );

        let mut blk = V1 {
            l1_batch_number: l1_batch_number.as_u64(),
            timestamp: timestamp.as_u64(),
            index_repeated_storage_changes: new_enumeration_index,
            new_state_root: state_root,
            number_of_l1_txs,
            l2_logs_tree_root,
            priority_operations_hash,
            initial_storage_changes: IndexMap::default(),
            repeated_storage_changes: IndexMap::default(),
            l2_logs: l2_logs.clone(),
            factory_deps: smartcontracts,
        };

        for initial_calldata in initial_changes_calldata[4..].chunks(64) {
            let key: [u8; 32] = initial_calldata[..32].try_into().unwrap();
            let value: [u8; 32] = initial_calldata[32..].try_into().unwrap();
            let key = U256::from_little_endian(&key);
            let _ = blk.initial_storage_changes.insert(key, value);
        }

        for repeated_calldata in repeated_changes_calldata[4..].chunks(40) {
            let index = u64::from_be_bytes([
                repeated_calldata[0],
                repeated_calldata[1],
                repeated_calldata[2],
                repeated_calldata[3],
                repeated_calldata[4],
                repeated_calldata[5],
                repeated_calldata[6],
                repeated_calldata[7],
            ]);
            let value: [u8; 32] = repeated_calldata[8..].try_into().unwrap();
            blk.repeated_storage_changes.insert(index, value);
        }

        Ok(blk)
    }
}

struct ExtractedToken {
    l1_batch_number: U256,
    timestamp: U256,
    new_enumeration_index: U256,
    state_root: Vec<u8>,
    number_of_l1_txs: U256,
    l2_logs_tree_root: Vec<u8>,
    priority_operations_hash: Vec<u8>,
    initial_changes_calldata: Vec<u8>,
    repeated_changes_calldata: Vec<u8>,
    l2_logs: Vec<u8>,
    factory_deps: Vec<ethabi::Token>,
}

impl TryFrom<&ethabi::Token> for ExtractedToken {
    type Error = ParseError;

    fn try_from(token: &ethabi::Token) -> Result<Self, Self::Error> {
        let ethabi::Token::Tuple(block_elems) = token else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "struct elements".to_string(),
            ));
        };

        let ethabi::Token::Uint(l1_batch_number) = block_elems[0].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "blockNumber".to_string(),
            ));
        };

        let ethabi::Token::Uint(timestamp) = block_elems[1].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo("timestamp".to_string()));
        };

        let ethabi::Token::Uint(new_enumeration_index) = block_elems[2].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "indexRepeatedStorageChanges".to_string(),
            ));
        };

        let ethabi::Token::FixedBytes(state_root) = block_elems[3].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "newStateRoot".to_string(),
            ));
        };

        let ethabi::Token::Uint(number_of_l1_txs) = block_elems[4].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "numberOfLayer1Txs".to_string(),
            ));
        };

        let ethabi::Token::FixedBytes(l2_logs_tree_root) = block_elems[5].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "l2LogsTreeRoot".to_string(),
            ));
        };

        let ethabi::Token::FixedBytes(priority_operations_hash) = block_elems[6].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "priorityOperationsHash".to_string(),
            ));
        };

        let ethabi::Token::Bytes(initial_changes_calldata) = block_elems[7].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "initialStorageChangesParam".to_string(),
            ));
        };

        if initial_changes_calldata.len() % 64 != 4 {
            return Err(ParseError::InvalidCommitBlockInfo(
                "initialStorageChangesLength".to_string(),
            ));
        }
        let ethabi::Token::Bytes(repeated_changes_calldata) = block_elems[8].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "repeatedStorageChanges".to_string(),
            ));
        };

        let ethabi::Token::Bytes(l2_logs) = block_elems[9].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo("l2Logs".to_string()));
        };

        // TODO(tuommaki): Are these useful at all?
        /*
        let ethabi::Token::Bytes(_l2_arbitrary_length_msgs) = block_elems[10].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "l2ArbitraryLengthMessages".to_string(),
            )
            );
        };
        */

        // TODO(tuommaki): Parse factory deps
        let ethabi::Token::Array(factory_deps) = block_elems[11].clone() else {
            return Err(ParseError::InvalidCommitBlockInfo(
                "factoryDeps".to_string(),
            ));
        };

        Ok(Self {
            l1_batch_number,
            timestamp,
            new_enumeration_index,
            state_root,
            number_of_l1_txs,
            l2_logs_tree_root,
            priority_operations_hash,
            initial_changes_calldata,
            repeated_changes_calldata,
            l2_logs,
            factory_deps,
        })
    }
}
