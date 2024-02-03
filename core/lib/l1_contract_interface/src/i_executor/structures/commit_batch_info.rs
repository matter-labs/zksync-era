use zksync_config::configs::chain::L1BatchCommitDataGeneratorMode;
use zksync_types::{
    commitment::L1BatchWithMetadata,
    ethabi::Token,
    utils,
    web3::{contract::Error as Web3ContractError, error::Error as Web3ApiError},
    U256,
};

use crate::Tokenizable;

/// Encoding for `CommitBatchInfo` from `IExecutor.sol`
#[derive(Debug)]
pub struct CommitBatchInfo<'a> {
    pub l1_batch_with_metadata: &'a L1BatchWithMetadata,
    pub l1_batch_commit_data_generator: L1BatchCommitDataGeneratorMode,
}

impl<'a> CommitBatchInfo<'a> {
    pub fn new(
        l1_batch_with_metadata: &'a L1BatchWithMetadata,
        l1_batch_commit_data_generator: L1BatchCommitDataGeneratorMode,
    ) -> Self {
        Self {
            l1_batch_with_metadata,
            l1_batch_commit_data_generator,
        }
    }
}

impl<'a> Tokenizable for CommitBatchInfo<'a> {
    fn from_token(_token: Token) -> Result<Self, Web3ContractError>
    where
        Self: Sized,
    {
        // Currently there is no need to decode this struct.
        // We still want to implement `Tokenizable` trait for it, so that *once* it's needed
        // the implementation is provided here and not in some other inconsistent way.
        Err(Web3ContractError::Api(Web3ApiError::Decoder(
            "Not implemented".to_string(),
        )))
    }

    fn into_token(self) -> Token {
        if self
            .l1_batch_with_metadata
            .header
            .protocol_version
            .unwrap()
            .is_pre_boojum()
        {
            pre_boojum_into_token(self.l1_batch_with_metadata)
        } else {
            match self.l1_batch_commit_data_generator {
                L1BatchCommitDataGeneratorMode::Rollup => {
                    Token::Tuple(rollup_mode_l1_commit_data(self.l1_batch_with_metadata))
                }
                L1BatchCommitDataGeneratorMode::Validium => {
                    Token::Tuple(validium_mode_l1_commit_data(self.l1_batch_with_metadata))
                }
            }
        }
    }
}

fn pre_boojum_into_token<'a>(l1_batch_commit_with_metadata: &'a L1BatchWithMetadata) -> Token {
    let header = &l1_batch_commit_with_metadata.header;
    let metadata = &l1_batch_commit_with_metadata.metadata;
    Token::Tuple(vec![
        Token::Uint(U256::from(header.number.0)),
        Token::Uint(U256::from(header.timestamp)),
        Token::Uint(U256::from(metadata.rollup_last_leaf_index)),
        Token::FixedBytes(metadata.merkle_root_hash.as_bytes().to_vec()),
        Token::Uint(U256::from(header.l1_tx_count)),
        Token::FixedBytes(metadata.l2_l1_merkle_root.as_bytes().to_vec()),
        Token::FixedBytes(header.priority_ops_onchain_data_hash().as_bytes().to_vec()),
        Token::Bytes(metadata.initial_writes_compressed.clone()),
        Token::Bytes(metadata.repeated_writes_compressed.clone()),
        Token::Bytes(metadata.l2_l1_messages_compressed.clone()),
        Token::Array(
            header
                .l2_to_l1_messages
                .iter()
                .map(|message| Token::Bytes(message.to_vec()))
                .collect(),
        ),
        Token::Array(
            l1_batch_commit_with_metadata
                .factory_deps
                .iter()
                .map(|bytecode| Token::Bytes(bytecode.to_vec()))
                .collect(),
        ),
    ])
}

fn validium_mode_l1_commit_data<'a>(l1_batch_with_metadata: &'a L1BatchWithMetadata) -> Vec<Token> {
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

fn rollup_mode_l1_commit_data<'a>(l1_batch_with_metadata: &'a L1BatchWithMetadata) -> Vec<Token> {
    let mut commit_data = validium_mode_l1_commit_data(l1_batch_with_metadata);
    commit_data.push(Token::Bytes(utils::construct_pubdata(
        l1_batch_with_metadata,
    )));
    commit_data
}
