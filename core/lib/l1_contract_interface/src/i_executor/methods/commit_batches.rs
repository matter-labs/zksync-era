use crate::{
    i_executor::structures::{get_encoding_version, CommitBatchInfo, StoredBatchInfo},
    Tokenizable, Tokenize,
};
use zksync_contracts::{
    hyperchain_contract, POST_BOOJUM_COMMIT_FUNCTION, POST_SHARED_BRIDGE_COMMIT_FUNCTION,
    POST_V26_GATEWAY_COMMIT_FUNCTION, PRE_BOOJUM_COMMIT_FUNCTION,
};
use zksync_types::ethabi::{Function, ParamType};
use zksync_types::web3::Bytes;
use zksync_types::{
    commitment::{L1BatchCommitmentMode, L1BatchWithMetadata},
    ethabi,
    ethabi::{encode, Token},
    pubdata_da::PubdataSendingMode,
    ProtocolVersionId,
};

/// Input required to encode `commitBatches` call for a contract
#[derive(Debug)]
pub struct CommitBatches<'a> {
    pub last_committed_l1_batch: &'a L1BatchWithMetadata,
    pub l1_batches: &'a [L1BatchWithMetadata],
    pub pubdata_da: PubdataSendingMode,
    pub mode: L1BatchCommitmentMode,
}

impl Tokenize for &CommitBatches<'_> {
    fn into_tokens(self) -> Vec<Token> {
        let protocol_version = self.l1_batches[0].header.protocol_version.unwrap();
        let stored_batch_info = StoredBatchInfo::from(self.last_committed_l1_batch)
            .into_token_with_protocol_version(protocol_version);
        let l1_batches_to_commit = self
            .l1_batches
            .iter()
            .map(|batch| CommitBatchInfo::new(self.mode, batch, self.pubdata_da).into_token())
            .collect();

        if protocol_version.is_pre_gateway() {
            vec![stored_batch_info, Token::Array(l1_batches_to_commit)]
        } else {
            let encoding_version = get_encoding_version(protocol_version);
            let mut encoded_data = encode(&[
                stored_batch_info.clone(),
                Token::Array(l1_batches_to_commit),
            ]);
            encoded_data.insert(0, encoding_version);
            vec![
                Token::Uint((self.last_committed_l1_batch.header.number.0 + 1).into()),
                Token::Uint(
                    (self.last_committed_l1_batch.header.number.0 + self.l1_batches.len() as u32)
                        .into(),
                ),
                Token::Bytes(encoded_data),
            ]
        }
    }
}

impl CommitBatches {
    fn function(protocol_version_id: ProtocolVersionId) -> Function {
        if protocol_version_id.is_pre_boojum() {
            *PRE_BOOJUM_COMMIT_FUNCTION.clone()
        } else if protocol_version_id.is_pre_shared_bridge() {
            *POST_BOOJUM_COMMIT_FUNCTION.clone()
        } else if protocol_version_id.is_pre_gateway() {
            *POST_SHARED_BRIDGE_COMMIT_FUNCTION.clone()
        } else if protocol_version_id.is_pre_interop_fast_blocks() {
            *POST_V26_GATEWAY_COMMIT_FUNCTION.clone()
        } else {
            hyperchain_contract()
                .function("commitBatchesSharedBridge")
                .unwrap()
                .clone()
            // .context("L1 contract does not have `commitBatchesSharedBridge` function")
            // .map_err(CheckError::Internal)?
        }
    }
}

pub fn decode_commit_function(
    bytes: Bytes,
    protocol_version: ProtocolVersionId,
) -> anyhow::Result<(StoredBatchInfo, Vec<CommitBatchInfo>)> {
    let function = CommitBatches::function(protocol_version);
    let mut tokens = function
        .decode_input(&bytes.0)
        .map_err(|err| format!("Failed to decode input: {:?}", err))?;

    if protocol_version.is_pre_gateway() {
        assert_eq!(tokens.len(), 2);
        let stored_batch = StoredBatchInfo::from_token(tokens.pop().unwrap(), protocol_version)?;
        let mut commit_batches = Vec::new();
        for batch in tokens.pop().unwrap().into_array().unwrap() {
            // let batch_info = CommitBatchInfo::from_token(batch, protocol_version)?;
            // commit_batches.push(batch_info);
        }
        Ok((stored_batch, commit_batches))
    } else if protocol_version.is_pre_interop_fast_blocks() {
        assert_eq!(tokens.len(), 3);
        let _chain_id_or_address = tokens.pop().unwrap();
        let stored_batch = StoredBatchInfo::from_token(tokens[0].clone(), protocol_version)?;
        let mut commit_batches = Vec::new();
        for batch in tokens[1].clone().into_array().unwrap() {
            // let batch_info = CommitBatchInfo::from_token(batch, protocol_version)?;
            // commit_batches.push(batch_info);
        }
        Ok((stored_batch, commit_batches))
    } else {
        assert_eq!(tokens.len(), 3);
        let _chain_id_or_address = tokens.pop().unwrap();
        let _process_from_index = tokens.pop().unwrap();
        let _process_to_index = tokens.pop().unwrap();
        let data = tokens.pop().unwrap();
        let decoded_data = ethabi::decode(
            &[
                StoredBatchInfo::schema_for_protocol_version(protocol_version),
                ParamType::Tuple(vec![CommitBatchInfo::post_gateway_schema()]),
            ],
            &data.into_bytes().unwrap()[1..], // skip the first byte (encoding version)
        );
        todo!()
        // let stored_batch = StoredBatchInfo::from_token(tokens[0].clone(), protocol_version)?;
        // let mut commit_batches = Vec::new();
        // for batch in tokens[1].clone().into_array().unwrap() {
        //     let batch_info = CommitBatchInfo::from_token(batch, protocol_version)?;
        //     commit_batches.push(batch_info);
        // }
        // Ok((stored_batch, commit_batches))
    }
}
