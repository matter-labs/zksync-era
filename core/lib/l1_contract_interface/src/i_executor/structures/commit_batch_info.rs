use zksync_types::{
    commitment::{
        pre_boojum_serialize_commitments, serialize_commitments, L1BatchCommitmentMode,
        L1BatchWithMetadata,
    },
    ethabi::Token,
    pubdata_da::PubdataDA,
    web3::contract::Error as ContractError,
    ProtocolVersionId, U256,
};

use crate::{
    i_executor::commit::kzg::{KzgInfo, ZK_SYNC_BYTES_PER_BLOB},
    Tokenizable,
};

/// These are used by the L1 Contracts to indicate what DA layer is used for pubdata
const PUBDATA_SOURCE_CALLDATA: u8 = 0;
const PUBDATA_SOURCE_BLOBS: u8 = 1;
const PUBDATA_SOURCE_CUSTOM: u8 = 2;

/// Encoding for `CommitBatchInfo` from `IExecutor.sol` for a contract running in rollup mode.
#[derive(Debug)]
pub struct CommitBatchInfo<'a> {
    mode: L1BatchCommitmentMode,
    l1_batch_with_metadata: &'a L1BatchWithMetadata,
    pubdata_da: PubdataDA,
}

impl<'a> CommitBatchInfo<'a> {
    pub fn new(
        mode: L1BatchCommitmentMode,
        l1_batch_with_metadata: &'a L1BatchWithMetadata,
        pubdata_da: PubdataDA,
    ) -> Self {
        Self {
            mode,
            l1_batch_with_metadata,
            pubdata_da,
        }
    }

    fn base_tokens(&self) -> Vec<Token> {
        if self
            .l1_batch_with_metadata
            .header
            .protocol_version
            .unwrap_or_else(ProtocolVersionId::last_potentially_undefined)
            .is_pre_boojum()
        {
            vec![
                Token::Uint(U256::from(self.l1_batch_with_metadata.header.number.0)),
                Token::Uint(U256::from(self.l1_batch_with_metadata.header.timestamp)),
                Token::Uint(U256::from(
                    self.l1_batch_with_metadata.metadata.rollup_last_leaf_index,
                )),
                Token::FixedBytes(
                    self.l1_batch_with_metadata
                        .metadata
                        .root_hash
                        .as_bytes()
                        .to_vec(),
                ),
                Token::Uint(U256::from(self.l1_batch_with_metadata.header.l1_tx_count)),
                Token::FixedBytes(
                    self.l1_batch_with_metadata
                        .metadata
                        .l2_l1_merkle_root
                        .as_bytes()
                        .to_vec(),
                ),
                Token::FixedBytes(
                    self.l1_batch_with_metadata
                        .header
                        .priority_ops_onchain_data_hash()
                        .as_bytes()
                        .to_vec(),
                ),
                Token::Bytes(
                    self.l1_batch_with_metadata
                        .metadata
                        .initial_writes_compressed
                        .clone()
                        .unwrap(),
                ),
                Token::Bytes(
                    self.l1_batch_with_metadata
                        .metadata
                        .repeated_writes_compressed
                        .clone()
                        .unwrap(),
                ),
                Token::Bytes(pre_boojum_serialize_commitments(
                    &self.l1_batch_with_metadata.header.l2_to_l1_logs,
                )),
                Token::Array(
                    self.l1_batch_with_metadata
                        .header
                        .l2_to_l1_messages
                        .iter()
                        .map(|message| Token::Bytes(message.to_vec()))
                        .collect(),
                ),
                Token::Array(
                    self.l1_batch_with_metadata
                        .raw_published_factory_deps
                        .iter()
                        .map(|bytecode| Token::Bytes(bytecode.to_vec()))
                        .collect(),
                ),
            ]
        } else {
            vec![
                // `batchNumber`
                Token::Uint(U256::from(self.l1_batch_with_metadata.header.number.0)),
                // `timestamp`
                Token::Uint(U256::from(self.l1_batch_with_metadata.header.timestamp)),
                // `indexRepeatedStorageChanges`
                Token::Uint(U256::from(
                    self.l1_batch_with_metadata.metadata.rollup_last_leaf_index,
                )),
                // `newStateRoot`
                Token::FixedBytes(
                    self.l1_batch_with_metadata
                        .metadata
                        .root_hash
                        .as_bytes()
                        .to_vec(),
                ),
                // `numberOfLayer1Txs`
                Token::Uint(U256::from(self.l1_batch_with_metadata.header.l1_tx_count)),
                // `priorityOperationsHash`
                Token::FixedBytes(
                    self.l1_batch_with_metadata
                        .header
                        .priority_ops_onchain_data_hash()
                        .as_bytes()
                        .to_vec(),
                ),
                // `bootloaderHeapInitialContentsHash`
                Token::FixedBytes(
                    self.l1_batch_with_metadata
                        .metadata
                        .bootloader_initial_content_commitment
                        .unwrap()
                        .as_bytes()
                        .to_vec(),
                ),
                // `eventsQueueStateHash`
                Token::FixedBytes(
                    self.l1_batch_with_metadata
                        .metadata
                        .events_queue_commitment
                        .unwrap()
                        .as_bytes()
                        .to_vec(),
                ),
                // `systemLogs`
                Token::Bytes(serialize_commitments(
                    &self.l1_batch_with_metadata.header.system_logs,
                )),
            ]
        }
    }

    fn pubdata_input(&self) -> Vec<u8> {
        self.l1_batch_with_metadata
            .header
            .pubdata_input
            .clone()
            .unwrap_or_else(|| self.l1_batch_with_metadata.construct_pubdata())
    }
}

impl Tokenizable for CommitBatchInfo<'_> {
    fn from_token(_token: Token) -> Result<Self, ContractError> {
        // Currently there is no need to decode this struct.
        // We still want to implement `Tokenizable` trait for it, so that *once* it's needed
        // the implementation is provided here and not in some other inconsistent way.
        Err(ContractError::Other("Not implemented".into()))
    }

    fn into_token(self) -> Token {
        let mut tokens = self.base_tokens();

        let protocol_version = self
            .l1_batch_with_metadata
            .header
            .protocol_version
            .unwrap_or_else(ProtocolVersionId::last_potentially_undefined);

        if protocol_version.is_pre_boojum() {
            return Token::Tuple(tokens);
        }

        if protocol_version.is_pre_1_4_2() {
            tokens.push(Token::Bytes(match self.mode {
                L1BatchCommitmentMode::Rollup => self.pubdata_input(),
                // Here we're not pushing any pubdata on purpose; no pubdata is sent in Validium mode.
                L1BatchCommitmentMode::Validium => vec![],
            }));
        } else {
            tokens.push(Token::Bytes(match (self.mode, self.pubdata_da) {
                // Here we're not pushing any pubdata on purpose; no pubdata is sent in Validium mode.
                (
                    L1BatchCommitmentMode::Validium,
                    PubdataDA::Calldata | PubdataDA::RelayedL2Calldata,
                ) => self
                    .l1_batch_with_metadata
                    .metadata
                    .state_diff_hash
                    .0
                    .into(),
                (L1BatchCommitmentMode::Validium, PubdataDA::Blobs) => self
                    .l1_batch_with_metadata
                    .metadata
                    .state_diff_hash
                    .0
                    .into(),

                (L1BatchCommitmentMode::Rollup, PubdataDA::Custom) => {
                    panic!("Custom pubdata DA is incompatible with Rollup mode")
                }
                (L1BatchCommitmentMode::Validium, PubdataDA::Custom) => {
                    vec![PUBDATA_SOURCE_CUSTOM]
                }

                (
                    L1BatchCommitmentMode::Rollup,
                    PubdataDA::Calldata | PubdataDA::RelayedL2Calldata,
                ) => {
                    let pubdata = self.pubdata_input();

                    let header = compose_header_for_l1_commit_rollup(
                        self.l1_batch_with_metadata
                            .metadata
                            .state_diff_hash
                            .0
                            .into(),
                        pubdata.clone(),
                    );

                    // We compute and add the blob commitment to the pubdata payload so that we can verify the proof
                    // even if we are not using blobs.
                    let pubdata = self.pubdata_input();
                    let blob_commitment = KzgInfo::new(&pubdata).to_blob_commitment();
                    std::iter::once(PUBDATA_SOURCE_CALLDATA)
                        .chain(pubdata)
                        .chain(blob_commitment)
                        .collect()
                }
                (L1BatchCommitmentMode::Rollup, PubdataDA::Blobs) => {
                    let pubdata = self.pubdata_input();
                    let pubdata_commitments =
                        pubdata.chunks(ZK_SYNC_BYTES_PER_BLOB).flat_map(|blob| {
                            let kzg_info = KzgInfo::new(blob);
                            kzg_info.to_pubdata_commitment()
                        });
                    std::iter::once(PUBDATA_SOURCE_BLOBS)
                        .chain(pubdata_commitments)
                        .collect()
                }
            }));
        }

        Token::Tuple(tokens)
    }
}
