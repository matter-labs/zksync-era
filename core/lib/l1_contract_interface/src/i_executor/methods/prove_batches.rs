use crypto_codegen::serialize_proof;
use zksync_prover_interface::outputs::L1BatchProofForL1;
use zksync_types::{
    commitment::L1BatchWithMetadata, ethabi::Token, web3::contract::tokens::Tokenizable, U256,
};

use crate::{i_executor::structures::StoredBatchInfo, Tokenize};

/// Input required to encode `proveBatches` call.
#[derive(Debug, Clone)]
pub struct ProveBatches {
    pub prev_l1_batch: L1BatchWithMetadata,
    pub l1_batches: Vec<L1BatchWithMetadata>,
    pub proofs: Vec<L1BatchProofForL1>,
    pub should_verify: bool,
}

impl Tokenize for ProveBatches {
    fn into_tokens(self) -> Vec<Token> {
        let prev_l1_batch = StoredBatchInfo(&self.prev_l1_batch).into_token();
        let batches_arg = self
            .l1_batches
            .iter()
            .map(|batch| StoredBatchInfo(batch).into_token())
            .collect();
        let batches_arg = Token::Array(batches_arg);

        if self.should_verify {
            // currently we only support submitting a single proof
            assert_eq!(self.proofs.len(), 1);
            assert_eq!(self.l1_batches.len(), 1);

            let L1BatchProofForL1 {
                aggregation_result_coords,
                scheduler_proof,
            } = self.proofs.first().unwrap();

            let (_, proof) = serialize_proof(scheduler_proof);

            let aggregation_result_coords = if self.l1_batches[0]
                .header
                .protocol_version
                .unwrap()
                .is_pre_boojum()
            {
                Token::Array(
                    aggregation_result_coords
                        .iter()
                        .map(|bytes| Token::Uint(U256::from_big_endian(bytes)))
                        .collect(),
                )
            } else {
                Token::Array(Vec::new())
            };
            let proof_input = Token::Tuple(vec![
                aggregation_result_coords,
                Token::Array(proof.into_iter().map(Token::Uint).collect()),
            ]);

            vec![prev_l1_batch, batches_arg, proof_input]
        } else {
            vec![
                prev_l1_batch,
                batches_arg,
                Token::Tuple(vec![Token::Array(vec![]), Token::Array(vec![])]),
            ]
        }
    }
}
