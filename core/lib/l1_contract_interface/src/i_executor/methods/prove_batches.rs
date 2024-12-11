use crypto_codegen::serialize_proof;
use zksync_prover_interface::outputs::L1BatchProofForL1;
use zksync_types::{
    commitment::L1BatchWithMetadata,
    ethabi::{encode, Token},
};

use crate::{
    i_executor::structures::{StoredBatchInfo, SUPPORTED_ENCODING_VERSION},
    Tokenizable, Tokenize,
};

/// Input required to encode `proveBatches` call.
#[derive(Debug, Clone)]
pub struct ProveBatches {
    pub prev_l1_batch: L1BatchWithMetadata,
    pub l1_batches: Vec<L1BatchWithMetadata>,
    pub proofs: Vec<L1BatchProofForL1>,
    pub should_verify: bool,
}

impl Tokenize for &ProveBatches {
    fn into_tokens(self) -> Vec<Token> {
        let prev_l1_batch_info = StoredBatchInfo::from(&self.prev_l1_batch).into_token();
        let batches_arg = self
            .l1_batches
            .iter()
            .map(|batch| StoredBatchInfo::from(batch).into_token())
            .collect();
        let batches_arg = Token::Array(batches_arg);
        let protocol_version = self.l1_batches[0].header.protocol_version.unwrap();

        if self.should_verify {
            // currently we only support submitting a single proof
            assert_eq!(self.proofs.len(), 1);
            assert_eq!(self.l1_batches.len(), 1);

            let L1BatchProofForL1 {
                scheduler_proof, ..
            } = self.proofs.first().unwrap();

            let (_, proof) = serialize_proof(scheduler_proof);

            if protocol_version.is_pre_gateway() {
                let proof_input = Token::Tuple(vec![
                    Token::Array(Vec::new()),
                    Token::Array(proof.into_iter().map(Token::Uint).collect()),
                ]);

                vec![prev_l1_batch_info, batches_arg, proof_input]
            } else {
                let proof_input = Token::Array(proof.into_iter().map(Token::Uint).collect());

                let encoded_data = encode(&[prev_l1_batch_info, batches_arg, proof_input]);
                let prove_data = [[SUPPORTED_ENCODING_VERSION].to_vec(), encoded_data]
                    .concat()
                    .to_vec();

                vec![
                    Token::Uint((self.prev_l1_batch.header.number.0 + 1).into()),
                    Token::Uint(
                        (self.prev_l1_batch.header.number.0 + self.l1_batches.len() as u32).into(),
                    ),
                    Token::Bytes(prove_data),
                ]
            }
        } else if protocol_version.is_pre_gateway() {
            vec![
                prev_l1_batch_info,
                batches_arg,
                Token::Tuple(vec![Token::Array(vec![]), Token::Array(vec![])]),
            ]
        } else {
            let encoded_data = encode(&[prev_l1_batch_info, batches_arg, Token::Array(vec![])]);
            let prove_data = [[SUPPORTED_ENCODING_VERSION].to_vec(), encoded_data]
                .concat()
                .to_vec();

            vec![
                Token::Uint((self.prev_l1_batch.header.number.0 + 1).into()),
                Token::Uint(
                    (self.prev_l1_batch.header.number.0 + self.l1_batches.len() as u32).into(),
                ),
                Token::Bytes(prove_data),
            ]
        }
    }
}
