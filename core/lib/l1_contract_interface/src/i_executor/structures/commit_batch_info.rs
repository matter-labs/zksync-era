use std::borrow::Cow;

use zksync_types::{
    commitment::{pre_boojum_serialize_commitments, serialize_commitments, L1BatchWithMetadata},
    ethabi,
    ethabi::Token,
    pubdata_da::PubdataDA,
    web3::{contract::Error as Web3ContractError, error::Error as Web3ApiError},
    ProtocolVersionId, U256,
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
pub struct CommitBatchInfo<'a> {
    pub l1_batch_with_metadata: &'a L1BatchWithMetadata,
    pub pubdata_da: PubdataDA,
}

impl<'a> CommitBatchInfo<'a> {
    pub fn new(l1_batch_with_metadata: &'a L1BatchWithMetadata, pubdata_da: PubdataDA) -> Self {
        Self {
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
                        .merkle_root_hash
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
                        .merkle_root_hash
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

impl CommitBatchInfo<'static> {
    /// Determines which DA source was used in the `reference` commitment. It's assumed that the commitment was created
    /// using `CommitBatchInfo::into_token()`.
    ///
    /// # Errors
    ///
    /// Returns an error if `reference` is malformed.
    pub fn detect_da(
        protocol_version: ProtocolVersionId,
        reference: &Token,
    ) -> Result<PubdataDA, Web3ContractError> {
        fn parse_error(message: impl Into<Cow<'static, str>>) -> Web3ContractError {
            Web3ContractError::Abi(ethabi::Error::Other(message.into()))
        }

        if protocol_version.is_pre_1_4_2() {
            return Ok(PubdataDA::Calldata);
        }

        let reference = match reference {
            Token::Tuple(tuple) => tuple,
            _ => {
                return Err(parse_error(format!(
                    "reference has unexpected shape; expected a tuple, got {reference:?}"
                )))
            }
        };
        let Some(last_reference_token) = reference.last() else {
            return Err(parse_error("reference commitment data is empty"));
        };

        let last_reference_token = match last_reference_token {
            Token::Bytes(bytes) => bytes,
            _ => return Err(parse_error(format!(
                "last reference token has unexpected shape; expected bytes, got {last_reference_token:?}"
            ))),
        };
        match last_reference_token.first() {
            Some(&byte) if byte == PUBDATA_SOURCE_CALLDATA => Ok(PubdataDA::Calldata),
            Some(&byte) if byte == PUBDATA_SOURCE_BLOBS => Ok(PubdataDA::Blobs),
            Some(&byte) => Err(parse_error(format!(
                "unexpected first byte of the last reference token; expected one of [{PUBDATA_SOURCE_CALLDATA}, {PUBDATA_SOURCE_BLOBS}], \
                 got {byte}"
            ))),
            None => Err(parse_error("last reference token is empty")),
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

        let protocol_version = self
            .l1_batch_with_metadata
            .header
            .protocol_version
            .unwrap_or_else(ProtocolVersionId::last_potentially_undefined);

        if protocol_version.is_pre_boojum() {
            return Token::Tuple(tokens);
        }

        if protocol_version.is_pre_1_4_2() {
            tokens.push(
                // `totalL2ToL1Pubdata` without pubdata source byte
                Token::Bytes(self.pubdata_input()),
            );
        } else {
            let pubdata = self.pubdata_input();
            match self.pubdata_da {
                PubdataDA::Calldata => {
                    // We compute and add the blob commitment to the pubdata payload so that we can verify the proof
                    // even if we are not using blobs.
                    let blob_commitment = KzgInfo::new(&pubdata).to_blob_commitment();

                    let result = std::iter::once(PUBDATA_SOURCE_CALLDATA)
                        .chain(pubdata)
                        .chain(blob_commitment)
                        .collect();

                    tokens.push(Token::Bytes(result));
                }
                PubdataDA::Blobs => {
                    let pubdata_commitments =
                        pubdata.chunks(ZK_SYNC_BYTES_PER_BLOB).flat_map(|blob| {
                            let kzg_info = KzgInfo::new(blob);
                            kzg_info.to_pubdata_commitment()
                        });
                    let result = std::iter::once(PUBDATA_SOURCE_BLOBS)
                        .chain(pubdata_commitments)
                        .collect();

                    tokens.push(Token::Bytes(result));
                }
            }
        }

        Token::Tuple(tokens)
    }
}
