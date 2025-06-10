use zksync_types::{
    commitment::{L1BatchCommitmentMode, L1BatchWithMetadata},
    ethabi::{encode, Token},
    pubdata_da::PubdataSendingMode,
};

use crate::{
    i_executor::structures::{
        CommitBatchInfo, StoredBatchInfo, PRE_INTEROP_ENCODING_VERSION, SUPPORTED_ENCODING_VERSION,
    },
    Tokenizable, Tokenize,
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
            let encoding_version = if protocol_version.is_pre_interop() {
                PRE_INTEROP_ENCODING_VERSION
            } else {
                SUPPORTED_ENCODING_VERSION
            };
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
