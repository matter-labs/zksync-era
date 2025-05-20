use zksync_types::web3::contract::Tokenizable;

use zksync_types::{
    commitment::{L1BatchWithMetadata, PriorityOpsMerkleProof},
    ethabi::{encode, Token},
    l2_to_l1_log::UserL2ToL1Log,
    InteropRoot, ProtocolVersionId, H256,
};

use crate::i_executor::structures::{
    StoredBatchInfo, PRE_INTEROP_ENCODING_VERSION, SUPPORTED_ENCODING_VERSION,
};

/// Input required to encode `executeBatches` call.
#[derive(Debug, Clone)]
pub struct ExecuteBatches {
    pub l1_batches: Vec<L1BatchWithMetadata>,
    pub priority_ops_proofs: Vec<PriorityOpsMerkleProof>,
    pub dependency_roots: Vec<Vec<InteropRoot>>,
    pub logs: Vec<Vec<UserL2ToL1Log>>,
    pub messages: Vec<Vec<Vec<u8>>>,
    pub message_roots: Vec<H256>, //
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
                    .map(|batch| {
                        StoredBatchInfo::from(batch)
                            .into_token_with_protocol_version(internal_protocol_version)
                    })
                    .collect(),
            )]
        } else if internal_protocol_version.is_pre_interop()
            && chain_protocol_version.is_pre_interop()
        {
            let encoded_data = encode(&[
                Token::Array(
                    self.l1_batches
                        .iter()
                        .map(|batch| {
                            StoredBatchInfo::from(batch)
                                .into_token_with_protocol_version(internal_protocol_version)
                        })
                        .collect(),
                ),
                Token::Array(
                    self.priority_ops_proofs
                        .iter()
                        .map(|proof| proof.into_token())
                        .collect(),
                ),
            ]);
            let execute_data = [[PRE_INTEROP_ENCODING_VERSION].to_vec(), encoded_data]
                .concat()
                .to_vec();

            vec![
                Token::Uint(self.l1_batches[0].header.number.0.into()),
                Token::Uint(self.l1_batches.last().unwrap().header.number.0.into()),
                Token::Bytes(execute_data),
            ]
        } else {
            let encoded_data = encode(&[
                Token::Array(
                    self.l1_batches
                        .iter()
                        .map(|batch| {
                            StoredBatchInfo::from(batch)
                                .into_token_with_protocol_version(internal_protocol_version)
                        })
                        .collect(),
                ),
                Token::Array(
                    self.priority_ops_proofs
                        .iter()
                        .map(|proof| proof.into_token())
                        .collect(),
                ),
                Token::Array(
                    self.dependency_roots
                        .iter()
                        .map(|batch_roots| {
                            Token::Array(
                                batch_roots
                                    .iter()
                                    .map(|root| root.clone().into_token())
                                    .collect(),
                            )
                        })
                        .collect(),
                ),
                Token::Array(
                    self.logs
                        .iter()
                        .map(|log| {
                            Token::Array(log.iter().map(|log| log.clone().0.into_token()).collect())
                        })
                        .collect(),
                ),
                Token::Array(
                    self.messages
                        .iter()
                        .map(|message| {
                            Token::Array(
                                message
                                    .iter()
                                    .map(|message| message.clone().
                                    into_token())
                                    .collect(),
                            )
                        })
                        .collect(),
                ),
                Token::Array(
                    self.message_roots
                        .iter()
                        .map(|root| Token::FixedBytes(root.0.as_slice().try_into().unwrap()))
                        .collect(),
                ),
            ]);
            let execute_data = [[SUPPORTED_ENCODING_VERSION].to_vec(), encoded_data]
                .concat()
                .to_vec(); //
            vec![
                Token::Uint(self.l1_batches[0].header.number.0.into()),
                Token::Uint(self.l1_batches.last().unwrap().header.number.0.into()),
                Token::Bytes(execute_data),
            ]
        }
    }
}
