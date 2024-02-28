use std::sync::Arc;

use zkevm_test_harness_1_4_2::kzg::KzgSettings;
use zksync_types::{
    commitment::{pre_boojum_serialize_commitments, serialize_commitments, L1BatchWithMetadata},
    ethabi::Token,
    pubdata_da::PubdataDA,
    web3::{contract::Error as Web3ContractError, error::Error as Web3ApiError},
    U256,
};

use crate::{
    i_executor::commit::kzg::{KzgInfo, ZK_SYNC_BYTES_PER_BLOB},
    Tokenizable,
};

/// These are used by the L1 Contracts to indicate what DA layer is used for pubdata
const PUBDATA_SOURCE_CALLDATA: u8 = 0;
const PUBDATA_SOURCE_BLOBS: u8 = 1;

/// Encoding for `CommitBatchInfo` from `IExecutor.sol`
#[derive(Debug)]
pub struct CommitBatchInfo<'a>(
    pub &'a L1BatchWithMetadata,
    pub Option<PubdataDA>,
    pub Option<Arc<KzgSettings>>,
);

impl CommitBatchInfo<'_> {
    fn base_tokens(&self) -> Vec<Token> {
        if self.0.header.protocol_version.unwrap().is_pre_boojum() {
            vec![
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
                Token::Bytes(self.0.metadata.initial_writes_compressed.clone().unwrap()),
                Token::Bytes(self.0.metadata.repeated_writes_compressed.clone().unwrap()),
                Token::Bytes(pre_boojum_serialize_commitments(
                    &self.0.header.l2_to_l1_logs,
                )),
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
                        .raw_published_factory_deps
                        .iter()
                        .map(|bytecode| Token::Bytes(bytecode.to_vec()))
                        .collect(),
                ),
            ]
        } else {
            vec![
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
                Token::Bytes(serialize_commitments(&self.0.header.system_logs)),
            ]
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
        let mut tokens = self.base_tokens();

        if !self.0.header.protocol_version.unwrap().is_pre_boojum() {
            match self.1 {
                None => tokens.push(
                    // `totalL2ToL1Pubdata` without pubdata source byte
                    Token::Bytes(
                        self.0
                            .header
                            .pubdata_input
                            .clone()
                            .unwrap_or(self.0.construct_pubdata()),
                    ),
                ),
                Some(pubdata_da) => match pubdata_da {
                    PubdataDA::Calldata => {
                        let pubdata = self
                            .0
                            .header
                            .pubdata_input
                            .clone()
                            .unwrap_or(self.0.construct_pubdata());

                        let blob_commitment =
                            KzgInfo::new(self.2.as_ref().unwrap(), &pubdata).to_blob_commitment();

                        let result = std::iter::once(PUBDATA_SOURCE_CALLDATA)
                            .chain(pubdata)
                            .chain(blob_commitment)
                            .collect();

                        tokens.push(Token::Bytes(result));
                    }
                    PubdataDA::Blobs => {
                        // `pubdataCommitments` with pubdata source byte
                        let pubdata = self
                            .0
                            .header
                            .pubdata_input
                            .clone()
                            .unwrap_or(self.0.construct_pubdata());

                        let mut pubdata_commitments = pubdata
                            .chunks(ZK_SYNC_BYTES_PER_BLOB)
                            .flat_map(|blob| {
                                let kzg_info = KzgInfo::new(self.2.as_ref().unwrap(), blob);
                                kzg_info.to_pubdata_commitment().to_vec()
                            })
                            .collect::<Vec<u8>>();
                        pubdata_commitments.insert(0, PUBDATA_SOURCE_BLOBS);
                        tokens.push(Token::Bytes(pubdata_commitments));
                    }
                },
            }
        }

        Token::Tuple(tokens)
    }
}
