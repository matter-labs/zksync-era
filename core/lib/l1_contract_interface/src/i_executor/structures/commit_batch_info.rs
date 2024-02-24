use once_cell::sync::Lazy;
use zkevm_test_harness_1_4_2::kzg::KzgSettings;
use zksync_types::{
    commitment::{pre_boojum_serialize_commitments, serialize_commitments, L1BatchWithMetadata},
    ethabi::Token,
    web3::{contract::Error as Web3ContractError, error::Error as Web3ApiError},
    U256,
};

use crate::{
    i_executor::commit::kzg::{KzgInfo, ZK_SYNC_BYTES_PER_BLOB},
    Tokenizable,
};

/// Encoding for `CommitBatchInfo` from `IExecutor.sol`
#[derive(Debug)]
pub struct CommitBatchInfo<'a>(pub &'a L1BatchWithMetadata);

pub fn load_kzg_settings() -> KzgSettings {
    static KZG_SETTINGS: Lazy<KzgSettings> = Lazy::new(|| {
        let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());
        let path = std::path::Path::new(&zksync_home).join("trusted_setup.json");
        KzgSettings::new(path.to_str().unwrap())
    });
    KZG_SETTINGS.clone()
}

impl CommitBatchInfo<'_> {
    fn into_token_base(&self) -> Vec<Token> {
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

    pub fn into_tokens_calldata(&self) -> Token {
        let mut tokens = self.into_token_base();
        let mut pubdata = self
            .0
            .header
            .pubdata_input
            .clone()
            .unwrap_or(self.0.construct_pubdata());

        let kzg_settings = load_kzg_settings();
        let mut blob = pubdata.clone();
        blob.resize(ZK_SYNC_BYTES_PER_BLOB, 0u8);
        let blob_commitment = KzgInfo::new(&kzg_settings, blob).to_blob_commitment();

        pubdata.extend(blob_commitment);

        pubdata.insert(0, 0);
        tokens.push(Token::Bytes(pubdata));
        Token::Tuple(tokens)
    }

    pub fn into_tokens_blobs(&self, number_of_blobs: usize) -> Token {
        let mut tokens = self.into_token_base();
        let kzg_settings = load_kzg_settings();
        let mut pubdata = self
            .0
            .header
            .pubdata_input
            .clone()
            .unwrap_or(self.0.construct_pubdata());
        pubdata.resize(ZK_SYNC_BYTES_PER_BLOB * number_of_blobs, 0u8);

        let mut pubdata_commitments = pubdata
            .chunks(ZK_SYNC_BYTES_PER_BLOB)
            .map(|blob| {
                let kzg_info = KzgInfo::new(&kzg_settings, blob.to_vec());
                kzg_info.to_pubdata_commitment().to_vec()
            })
            .flatten()
            .collect::<Vec<u8>>();
        pubdata_commitments.insert(0, 1u8);
        tokens.push(Token::Bytes(pubdata_commitments));
        Token::Tuple(tokens)
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
        let mut tokens = self.into_token_base();
        if !self.0.header.protocol_version.unwrap().is_pre_boojum() {
            tokens.push(
                // `totalL2ToL1Pubdata`
                Token::Bytes(
                    self.0
                        .header
                        .pubdata_input
                        .clone()
                        .unwrap_or(self.0.construct_pubdata()),
                ),
            );
        }

        Token::Tuple(tokens)
    }
}
