use zksync_types::{
    commitment::{L1BatchWithMetadata, PriorityOpsMerkleProof},
    ethabi::{encode, Token},
    ProtocolVersionId,
};

use crate::{
    i_executor::structures::{StoredBatchInfo, SUPPORTED_ENCODING_VERSION},
    Tokenizable,
};

/// Input required to encode `executeBatches` call.
#[derive(Debug, Clone)]
pub struct ExecuteBatches {
    pub l1_batches: Vec<L1BatchWithMetadata>,
    pub priority_ops_proofs: Vec<PriorityOpsMerkleProof>,
}

impl ExecuteBatches {
    // The encodings of `ExecuteBatches` operations are different depending on the protocol version
    // of the underlying chain.
    // However, we can send batches with older protocol versions just by changing the encoding.
    // This makes the migration simpler.
    pub fn encode_for_eth_tx(&self, chain_protocol_version: ProtocolVersionId) -> Vec<Token> {
        let internal_protocol_version = self.l1_batches[0].header.protocol_version.unwrap();

        if internal_protocol_version.is_pre_gateway() && chain_protocol_version.is_pre_gateway() {
            vec![Token::Array(
                self.l1_batches
                    .iter()
                    .map(|batch| StoredBatchInfo::from(batch).into_token())
                    .collect(),
            )]
        } else {
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
            let execute_data = [[SUPPORTED_ENCODING_VERSION].to_vec(), encoded_data]
                .concat()
                .to_vec();

            vec![
                Token::Uint(self.l1_batches[0].header.number.0.into()),
                Token::Uint(self.l1_batches.last().unwrap().header.number.0.into()),
                Token::Bytes(execute_data),
            ]
        }
    }
}
