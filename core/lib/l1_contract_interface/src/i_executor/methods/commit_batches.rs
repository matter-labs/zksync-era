use zksync_types::{
    commitment::{L1BatchCommitmentMode, L1BatchWithMetadata},
    ethabi::{encode, Token},
    pubdata_da::PubdataDA,
};

use crate::{
    i_executor::structures::{CommitBatchInfo, StoredBatchInfo, SUPPORTED_ENCODING_VERSION},
    Tokenizable,
};
/// Input required to encode `commitBatches` call for a contract
#[derive(Debug)]
pub struct CommitBatches<'a> {
    pub last_committed_l1_batch: &'a L1BatchWithMetadata,
    pub l1_batches: &'a [L1BatchWithMetadata],
    pub pubdata_da: PubdataDA,
    pub mode: L1BatchCommitmentMode,
}

impl CommitBatches<'_> {
    pub fn into_tokens(self, pre_gateway:bool) -> Vec<Token> {
        let stored_batch_info = StoredBatchInfo::from(self.last_committed_l1_batch).into_token();
        let l1_batches_to_commit: Vec<Token> = self
            .l1_batches
            .iter()
            .map(|batch| CommitBatchInfo::new(self.mode, batch, self.pubdata_da).into_token())
            .collect();

        let encoded_data = encode(&[stored_batch_info.clone(), Token::Array(l1_batches_to_commit.clone())]);
        let commit_data = [[SUPPORTED_ENCODING_VERSION].to_vec(), encoded_data]
            .concat()
            .to_vec();
        if pre_gateway {
            vec![stored_batch_info, Token::Array(l1_batches_to_commit)]
        } else {
            vec![
                Token::Uint((self.last_committed_l1_batch.header.number.0 + 1).into()),
                Token::Uint(
                    (self.last_committed_l1_batch.header.number.0 + self.l1_batches.len() as u32)
                        .into(),
                ),
                Token::Bytes(commit_data),
            ]
        }
    }
}
