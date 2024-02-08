use zksync_basic_types::{ethabi::Token, U256};

use crate::{commitment::L1BatchWithMetadata, utils};

pub trait L1BatchCommitDataGenerator
where
    Self: std::fmt::Debug + Send + Sync,
{
    fn l1_commit_data(&self, l1_batch_with_metadata: &L1BatchWithMetadata) -> Token;
}

#[derive(Debug, Clone)]
pub struct RollupModeL1BatchCommitDataGenerator;

#[derive(Debug, Clone)]
pub struct ValidiumModeL1BatchCommitDataGenerator;

impl L1BatchCommitDataGenerator for RollupModeL1BatchCommitDataGenerator {
    fn l1_commit_data(&self, l1_batch_with_metadata: &L1BatchWithMetadata) -> Token {
        Token::Tuple(rollup_mode_l1_commit_data(l1_batch_with_metadata))
    }
}

impl L1BatchCommitDataGenerator for ValidiumModeL1BatchCommitDataGenerator {
    fn l1_commit_data(&self, l1_batch_with_metadata: &L1BatchWithMetadata) -> Token {
        Token::Tuple(validium_mode_l1_commit_data(l1_batch_with_metadata))
    }
}

fn validium_mode_l1_commit_data(l1_batch_with_metadata: &L1BatchWithMetadata) -> Vec<Token> {
    let header = &l1_batch_with_metadata.header;
    let metadata = &l1_batch_with_metadata.metadata;
    let commit_data = vec![
        // `batchNumber`
        Token::Uint(U256::from(header.number.0)),
        // `timestamp`
        Token::Uint(U256::from(header.timestamp)),
        // `indexRepeatedStorageChanges`
        Token::Uint(U256::from(metadata.rollup_last_leaf_index)),
        // `newStateRoot`
        Token::FixedBytes(metadata.merkle_root_hash.as_bytes().to_vec()),
        // `numberOfLayer1Txs`
        Token::Uint(U256::from(header.l1_tx_count)),
        // `priorityOperationsHash`
        Token::FixedBytes(header.priority_ops_onchain_data_hash().as_bytes().to_vec()),
        // `bootloaderHeapInitialContentsHash`
        Token::FixedBytes(
            metadata
                .bootloader_initial_content_commitment
                .unwrap()
                .as_bytes()
                .to_vec(),
        ),
        // `eventsQueueStateHash`
        Token::FixedBytes(
            metadata
                .events_queue_commitment
                .unwrap()
                .as_bytes()
                .to_vec(),
        ),
        // `systemLogs`
        Token::Bytes(metadata.l2_l1_messages_compressed.clone()),
    ];
    commit_data
}

fn rollup_mode_l1_commit_data(l1_batch_with_metadata: &L1BatchWithMetadata) -> Vec<Token> {
    let mut commit_data = validium_mode_l1_commit_data(l1_batch_with_metadata);
    commit_data.push(Token::Bytes(utils::construct_pubdata(
        l1_batch_with_metadata,
    )));
    commit_data
}
