use zksync_types::{commitment::L1BatchWithMetadata, ethabi::Token, U256};

use crate::IntoTokens;

/// Encoding for `CommitBatchInfo` from `IExecutor.sol`
#[derive(Debug)]
pub struct CommitBatchInfo<'a>(pub &'a L1BatchWithMetadata);

impl<'a> IntoTokens for CommitBatchInfo<'a> {
    fn into_tokens(self) -> Token {
        if self.0.header.protocol_version.unwrap().is_pre_boojum() {
            Token::Tuple(vec![
                Token::Uint(U256::from(self.0.header.number.0)),
                Token::Uint(U256::from(self.0.header.timestamp)),
                Token::Uint(U256::from(self.0.metadata.rollup_last_leaf_index)),
                Token::FixedBytes(self.0.metadata.merkle_root_hash.as_bytes().to_vec()),
                Token::Uint(U256::from(self.0.header.l1_tx_count)),
                Token::FixedBytes(self.0.metadata.l2_l1_merkle_root.as_bytes().to_vec()),
                Token::FixedBytes(
                    self.0
                        .header
                        .priority_ops_onchain_data_hash()
                        .as_bytes()
                        .to_vec(),
                ),
                Token::Bytes(self.0.metadata.initial_writes_compressed.clone()),
                Token::Bytes(self.0.metadata.repeated_writes_compressed.clone()),
                Token::Bytes(self.0.metadata.l2_l1_messages_compressed.clone()),
                Token::Array(
                    self.0
                        .header
                        .l2_to_l1_messages
                        .iter()
                        .map(|message| Token::Bytes(message.to_vec()))
                        .collect(),
                ),
                Token::Array(
                    self.0
                        .factory_deps
                        .iter()
                        .map(|bytecode| Token::Bytes(bytecode.to_vec()))
                        .collect(),
                ),
            ])
        } else {
            Token::Tuple(vec![
                // `batchNumber`
                Token::Uint(U256::from(self.0.header.number.0)),
                // `timestamp`
                Token::Uint(U256::from(self.0.header.timestamp)),
                // `indexRepeatedStorageChanges`
                Token::Uint(U256::from(self.0.metadata.rollup_last_leaf_index)),
                // `newStateRoot`
                Token::FixedBytes(self.0.metadata.merkle_root_hash.as_bytes().to_vec()),
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
                // `bootloaderHeapInitialContentsHash`
                Token::FixedBytes(
                    self.0
                        .metadata
                        .bootloader_initial_content_commitment
                        .unwrap()
                        .as_bytes()
                        .to_vec(),
                ),
                // `eventsQueueStateHash`
                Token::FixedBytes(
                    self.0
                        .metadata
                        .events_queue_commitment
                        .unwrap()
                        .as_bytes()
                        .to_vec(),
                ),
                // `systemLogs`
                Token::Bytes(self.0.metadata.l2_l1_messages_compressed.clone()),
                // `totalL2ToL1Pubdata`
                Token::Bytes(
                    self.0
                        .header
                        .pubdata_input
                        .clone()
                        .unwrap_or(self.0.construct_pubdata()),
                ),
            ])
        }
    }
}
