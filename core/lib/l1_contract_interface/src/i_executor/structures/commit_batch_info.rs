use std::sync::Arc;

use zkevm_test_harness_1_4_2::kzg::KzgSettings;
use zksync_types::{
    commitment::{serialize_commitments, L1BatchWithMetadata},
    ethabi::Token,
    pubdata_da::PubdataDA,
    web3::{contract::Error as Web3ContractError, error::Error as Web3ApiError},
    U256,
};

use crate::{
    i_executor::commit::kzg::{right_pad_pubdata_to_blobs, KzgInfo, ZK_SYNC_BYTES_PER_BLOB},
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
                    // `totalL2ToL1Pubdata` with pubdata source byte
                    let mut pubdata = vec![PUBDATA_SOURCE_CALLDATA];
                    pubdata.extend_from_slice(
                        &self
                            .0
                            .header
                            .pubdata_input
                            .as_ref()
                            .unwrap_or(&self.0.construct_pubdata()),
                    );

                    let mut blob = pubdata.clone();
                    right_pad_pubdata_to_blobs(&mut blob);
                    let blob_commitment =
                        KzgInfo::new(&self.2.as_ref().unwrap(), &blob).to_blob_commitment();

                    pubdata.extend(blob_commitment);

                    tokens.push(Token::Bytes(pubdata));
                }
                PubdataDA::Blobs => {
                    // `pubdataCommitments` with pubdata source byte
                    let mut pubdata = self
                        .0
                        .header
                        .pubdata_input
                        .clone()
                        .unwrap_or(self.0.construct_pubdata());
                    right_pad_pubdata_to_blobs(&mut pubdata);

                    let mut pubdata_commitments = pubdata
                        .chunks(ZK_SYNC_BYTES_PER_BLOB)
                        .flat_map(|blob| {
                            let kzg_info = KzgInfo::new(&self.2.as_ref().unwrap(), &blob);
                            kzg_info.to_pubdata_commitment().to_vec()
                        })
                        .collect::<Vec<u8>>();
                    pubdata_commitments.insert(0, PUBDATA_SOURCE_BLOBS);
                    tokens.push(Token::Bytes(pubdata_commitments));
                }
            },
        }

        Token::FixedArray(tokens)
    }
}
