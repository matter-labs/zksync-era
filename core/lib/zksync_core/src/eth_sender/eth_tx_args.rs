use codegen::serialize_proof;
use zksync_types::{
    aggregated_operations::{
        L1BatchCommitOperation, L1BatchExecuteOperation, L1BatchProofForL1, L1BatchProofOperation,
    },
    commitment::L1BatchWithMetadata,
    ethabi::Token,
    U256,
};

/// Trait for the types that can be encuded into the Ethereum transaction input.
pub(super) trait EthTxArgs {
    /// Formats the arguments into the Ethereum transaction input.
    fn get_eth_tx_args(&self) -> Vec<Token>;
}

impl EthTxArgs for L1BatchCommitOperation {
    fn get_eth_tx_args(&self) -> Vec<Token> {
        let stored_batch_info = self.last_committed_l1_batch.l1_header_data();
        let l1_batches_to_commit = self
            .l1_batches
            .iter()
            .map(L1BatchWithMetadata::l1_commit_data)
            .collect();

        vec![stored_batch_info, Token::Array(l1_batches_to_commit)]
    }
}

impl EthTxArgs for L1BatchProofOperation {
    fn get_eth_tx_args(&self) -> Vec<Token> {
        let prev_l1_batch = self.prev_l1_batch.l1_header_data();
        let batches_arg = self
            .l1_batches
            .iter()
            .map(L1BatchWithMetadata::l1_header_data)
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

impl EthTxArgs for L1BatchExecuteOperation {
    fn get_eth_tx_args(&self) -> Vec<Token> {
        vec![Token::Array(
            self.l1_batches
                .iter()
                .map(L1BatchWithMetadata::l1_header_data)
                .collect(),
        )]
    }
}
