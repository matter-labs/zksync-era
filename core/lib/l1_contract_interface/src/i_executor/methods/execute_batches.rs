use zksync_types::{
    commitment::{L1BatchWithMetadata, PriorityOpsMerkleProof},
    ethabi::{encode, Token},
};

use crate::{i_executor::structures::StoredBatchInfo, Tokenizable, Tokenize};
const SUPPORTED_ENCODING_VERSION: [u8; 1] = [0];

/// Input required to encode `executeBatches` call.
#[derive(Debug, Clone)]
pub struct ExecuteBatches {
    pub l1_batches: Vec<L1BatchWithMetadata>,
    pub priority_ops_proofs: Vec<PriorityOpsMerkleProof>,
}

impl Tokenize for &ExecuteBatches {
    fn into_tokens(self) -> Vec<Token> {
        let encoded_data = encode(&[
            Token::Array(
                self.l1_batches
                    .iter()
                    .map(|batch| StoredBatchInfo::from(batch).into_token())
                    .collect(),
            ),
            Token::Array(
                self.priority_ops_proofs
                    .iter()
                    .map(|proof| proof.into_token())
                    .collect(),
            ),
        ]);
        let commit_data = [SUPPORTED_ENCODING_VERSION.to_vec(), encoded_data]
            .concat()
            .to_vec();

        vec![
            Token::Uint((self.l1_batches[0].header.number.0).into()),
            Token::Uint((self.l1_batches[self.l1_batches.len() - 1].header.number.0).into()),
            Token::Bytes(commit_data),
        ]
    }
}
