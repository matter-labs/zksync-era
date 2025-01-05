use zksync_types::{
    commitment::{
        pre_boojum_serialize_commitments, serialize_commitments, L1BatchCommitmentMode,
        L1BatchWithMetadata,
    },
    ethabi::{ParamType, Token},
    pubdata_da::PubdataSendingMode,
    web3::{contract::Error as ContractError, keccak256},
    ProtocolVersionId, H256, U256,
};

use crate::{
    i_executor::commit::kzg::{KzgInfo, ZK_SYNC_BYTES_PER_BLOB},
    Tokenizable,
};

/// These are used by the L1 Contracts to indicate what DA layer is used for pubdata
pub const PUBDATA_SOURCE_CALLDATA: u8 = 0;
pub const PUBDATA_SOURCE_BLOBS: u8 = 1;
pub const PUBDATA_SOURCE_CUSTOM_PRE_GATEWAY: u8 = 2;

/// Encoding for `CommitBatchInfo` from `IExecutor.sol` for a contract running in rollup mode.
#[derive(Debug)]
pub struct CommitBatchInfo<'a> {
    mode: L1BatchCommitmentMode,
    l1_batch_with_metadata: &'a L1BatchWithMetadata,
    pubdata_da: PubdataSendingMode,
}

impl<'a> CommitBatchInfo<'a> {
    pub fn new(
        mode: L1BatchCommitmentMode,
        l1_batch_with_metadata: &'a L1BatchWithMetadata,
        pubdata_da: PubdataSendingMode,
    ) -> Self {
        Self {
            mode,
            l1_batch_with_metadata,
            pubdata_da,
        }
    }

    pub fn post_gateway_schema() -> ParamType {
        ParamType::Tuple(vec![
            ParamType::Uint(64),       // `batch_number`
            ParamType::Uint(64),       // `timestamp`
            ParamType::Uint(64),       // `index_repeated_storage_changes`
            ParamType::FixedBytes(32), // `new_state_root`
            ParamType::Uint(256),      // `numberOfLayer1Txs`
            ParamType::FixedBytes(32), // `priorityOperationsHash`
            ParamType::FixedBytes(32), // `bootloaderHeapInitialContentsHash`
            ParamType::FixedBytes(32), // `eventsQueueStateHash`
            ParamType::Bytes,          // `systemLogs`
            ParamType::Bytes,          // `operatorDAInput`
        ])
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
        } else if protocol_version.is_pre_gateway() {
            tokens.push(Token::Bytes(match (self.mode, self.pubdata_da) {
                // Here we're not pushing any pubdata on purpose; no pubdata is sent in Validium mode.
                (
                    L1BatchCommitmentMode::Validium,
                    PubdataSendingMode::Calldata | PubdataSendingMode::RelayedL2Calldata,
                ) => {
                    vec![PUBDATA_SOURCE_CALLDATA]
                }
                (L1BatchCommitmentMode::Validium, PubdataSendingMode::Blobs) => {
                    vec![PUBDATA_SOURCE_BLOBS]
                }
                (L1BatchCommitmentMode::Rollup, PubdataSendingMode::Custom) => {
                    panic!("Custom pubdata DA is incompatible with Rollup mode")
                }
                (L1BatchCommitmentMode::Validium, PubdataSendingMode::Custom) => {
                    vec![PUBDATA_SOURCE_CUSTOM_PRE_GATEWAY]
                }
                (
                    L1BatchCommitmentMode::Rollup,
                    PubdataSendingMode::Calldata | PubdataSendingMode::RelayedL2Calldata,
                ) => {
                    // We compute and add the blob commitment to the pubdata payload so that we can verify the proof
                    // even if we are not using blobs.
                    let pubdata = self.pubdata_input();
                    let blob_commitment = KzgInfo::new(&pubdata).to_blob_commitment();
                    [PUBDATA_SOURCE_CALLDATA]
                        .into_iter()
                        .chain(pubdata)
                        .chain(blob_commitment)
                        .collect()
                }
                (L1BatchCommitmentMode::Rollup, PubdataSendingMode::Blobs) => {
                    let pubdata = self.pubdata_input();
                    let pubdata_commitments =
                        pubdata.chunks(ZK_SYNC_BYTES_PER_BLOB).flat_map(|blob| {
                            let kzg_info = KzgInfo::new(blob);
                            kzg_info.to_pubdata_commitment()
                        });
                    [PUBDATA_SOURCE_BLOBS]
                        .into_iter()
                        .chain(pubdata_commitments)
                        .collect()
                }
            }));
        } else {
            let state_diff_hash = self
                .l1_batch_with_metadata
                .metadata
                .state_diff_hash
                .expect("Failed to get state_diff_hash from metadata");
            tokens.push(Token::Bytes(match (self.mode, self.pubdata_da) {
                // Validiums with custom DA need the inclusion data to be part of operator_da_input
                (L1BatchCommitmentMode::Validium, PubdataSendingMode::Custom) => {
                    let mut operator_da_input: Vec<u8> = state_diff_hash.0.into();

                    operator_da_input.extend(
                        &self
                            .l1_batch_with_metadata
                            .metadata
                            .da_inclusion_data
                            .clone()
                            .unwrap_or_default(),
                    );

                    operator_da_input
                }
                // Here we're not pushing any pubdata on purpose; no pubdata is sent in Validium mode.
                (
                    L1BatchCommitmentMode::Validium,
                    PubdataSendingMode::Calldata
                    | PubdataSendingMode::RelayedL2Calldata
                    | PubdataSendingMode::Blobs,
                ) => state_diff_hash.0.into(),
                (L1BatchCommitmentMode::Rollup, PubdataSendingMode::Custom) => {
                    panic!("Custom pubdata DA is incompatible with Rollup mode")
                }
                (L1BatchCommitmentMode::Rollup, PubdataSendingMode::Calldata) => {
                    let pubdata = self.pubdata_input();

                    let header =
                        compose_header_for_l1_commit_rollup(state_diff_hash, pubdata.clone());

                    // We compute and add the blob commitment to the pubdata payload so that we can verify the proof
                    // even if we are not using blobs.
                    let blob_commitment = KzgInfo::new(&pubdata).to_blob_commitment();
                    header
                        .into_iter()
                        .chain([PUBDATA_SOURCE_CALLDATA])
                        .chain(pubdata)
                        .chain(blob_commitment)
                        .collect()
                }
                (L1BatchCommitmentMode::Rollup, PubdataSendingMode::RelayedL2Calldata) => {
                    let pubdata = self.pubdata_input();

                    let header =
                        compose_header_for_l1_commit_rollup(state_diff_hash, pubdata.clone());

                    // We compute and add the blob commitment to the pubdata payload so that we can verify the proof
                    // even if we are not using blobs.
                    // Note, that the only difference from the `PubdataSendingMode::Calldata` is that mutliple
                    // commitments can be provided as the size of a transaction on top of Gateway can exceed 120kb.
                    let blob_commitments: Vec<u8> = if pubdata.is_empty() {
                        // For consistency with `Calldata` sending mode we provide some non-empty commitments
                        // even when pubdata is zero. Note, that this will never actually happen in production,
                        // but this helps for easier testing.
                        KzgInfo::new(&pubdata).to_blob_commitment().to_vec()
                    } else {
                        pubdata
                            .chunks(ZK_SYNC_BYTES_PER_BLOB)
                            .flat_map(|blob| {
                                let blob_commitment = KzgInfo::new(blob).to_blob_commitment();

                                // We also append 0s to show that we do not reuse previously published blobs.
                                blob_commitment.into_iter().collect::<Vec<u8>>()
                            })
                            .collect()
                    };

                    header
                        .into_iter()
                        .chain([PUBDATA_SOURCE_CALLDATA])
                        .chain(pubdata)
                        .chain(blob_commitments)
                        .collect()
                }
                (L1BatchCommitmentMode::Rollup, PubdataSendingMode::Blobs) => {
                    let pubdata = self.pubdata_input();

                    let header =
                        compose_header_for_l1_commit_rollup(state_diff_hash, pubdata.clone());

                    let pubdata_commitments: Vec<u8> = pubdata
                        .chunks(ZK_SYNC_BYTES_PER_BLOB)
                        .flat_map(|blob| {
                            let kzg_info = KzgInfo::new(blob);

                            let blob_commitment = kzg_info.to_pubdata_commitment();

                            // We also append 0s to show that we do not reuse previously published blobs.
                            blob_commitment
                                .into_iter()
                                .chain([0u8; 32])
                                .collect::<Vec<u8>>()
                        })
                        .collect();
                    header
                        .into_iter()
                        .chain([PUBDATA_SOURCE_BLOBS])
                        .chain(pubdata_commitments)
                        .collect()
                }
            }));
        }

        Token::Tuple(tokens)
    }
}

fn compose_header_for_l1_commit_rollup(state_diff_hash: H256, pubdata: Vec<u8>) -> Vec<u8> {
    // The preimage under the hash `l2DAValidatorOutputHash` is expected to be in the following format:
    // - First 32 bytes are the hash of the uncompressed state diff.
    // - Then, there is a 32-byte hash of the full pubdata.
    // - Then, there is the 1-byte number of blobs published.
    // - Then, there are linear hashes of the published blobs, 32 bytes each.

    let mut full_header = vec![];

    full_header.extend(state_diff_hash.0);

    let mut full_pubdata = pubdata;
    let full_pubdata_hash = keccak256(&full_pubdata);
    full_header.extend(full_pubdata_hash);

    // Now, we need to calculate the linear hashes of the blobs.
    // Firstly, let's pad the pubdata to the size of the blob.
    if full_pubdata.len() % ZK_SYNC_BYTES_PER_BLOB != 0 {
        full_pubdata.resize(
            full_pubdata.len() + ZK_SYNC_BYTES_PER_BLOB
                - full_pubdata.len() % ZK_SYNC_BYTES_PER_BLOB,
            0,
        );
    }
    full_header.push((full_pubdata.len() / ZK_SYNC_BYTES_PER_BLOB) as u8);

    full_pubdata
        .chunks(ZK_SYNC_BYTES_PER_BLOB)
        .for_each(|chunk| {
            full_header.extend(keccak256(chunk));
        });

    full_header
}
