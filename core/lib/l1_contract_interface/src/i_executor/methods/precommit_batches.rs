use zksync_types::{
    ethabi::{encode, Token},
    transaction_status_commitment::TransactionStatusCommitment,
    L1BatchNumber, L2BlockNumber,
};

use crate::{i_executor::structures::EncodingVersion, Tokenize};

/// Input required to encode `preCommitBatches` call for a contract
#[derive(Debug)]
pub struct PrecommitBatches<'a> {
    pub txs: &'a [TransactionStatusCommitment],
    pub last_l2_block: L2BlockNumber,
    pub l1_batch_number: L1BatchNumber,
}

impl Tokenize for &PrecommitBatches<'_> {
    fn into_tokens(self) -> Vec<Token> {
        let packed_txs = self
            .txs
            .iter()
            .flat_map(|tx| tx.get_packed_bytes().into_iter())
            .collect::<Vec<_>>();

        let mut encoded_data = encode(&[Token::Tuple(vec![
            Token::Bytes(packed_txs),
            Token::Uint(self.last_l2_block.0.into()),
        ])]);
        encoded_data.insert(0, EncodingVersion::InteropSupported.value());
        vec![
            Token::Uint(self.l1_batch_number.0.into()),
            Token::Bytes(encoded_data),
        ]
    }
}
