use anyhow::Context;
use zksync_types::{
    commitment::{L1BatchCommitmentMode, L1BatchWithMetadata},
    ethabi,
    ethabi::{encode, ParamType, Token},
    pubdata_da::PubdataSendingMode,
    web3::contract::{Error, Error as ContractError},
    L1BatchNumber, L2BlockNumber, H256,
};

use crate::{
    i_executor::structures::{CommitBatchInfo, StoredBatchInfo, SUPPORTED_ENCODING_VERSION},
    Tokenizable, Tokenize,
};

#[derive(Debug, Clone)]
pub struct TransactionStatusCommitment {
    pub tx_hash: H256,
    pub status: bool,
}

/// Input required to encode `preCommitBatches` call for a contract
#[derive(Debug)]
pub struct PrecommitBatches<'a> {
    pub txs: &'a [TransactionStatusCommitment],
    pub last_l2_block: L2BlockNumber,
    pub l1_batch_number: L1BatchNumber,
}

impl Tokenizable for TransactionStatusCommitment {
    fn from_token(token: Token) -> Result<Self, ContractError> {
        (|| {
            let [Token::FixedBytes(tx_hash), Token::Bool(status)]: [Token; 2] = token
                .into_tuple()
                .context("not a tuple")?
                .try_into()
                .ok()
                .context("bad length")?
            else {
                anyhow::bail!("bad format")
            };
            Ok(Self {
                tx_hash: H256::from_slice(tx_hash.as_slice()),
                status,
            })
        })()
        .map_err(|err| ContractError::InvalidOutputType(format!("{err:#}")))
    }

    fn into_token(self) -> Token {
        Token::Tuple(vec![
            Token::FixedBytes(self.tx_hash.0.to_vec()),
            Token::Bool(self.status),
        ])
    }
}

impl Tokenize for &PrecommitBatches<'_> {
    fn into_tokens(self) -> Vec<Token> {
        let txs = self
            .txs
            .into_iter()
            .map(|tx| tx.clone().into_token())
            .collect::<Vec<_>>();

        let mut encoded_data =
            encode(&[Token::Array(txs), Token::Uint(self.last_l2_block.0.into())]);
        encoded_data.insert(0, SUPPORTED_ENCODING_VERSION);
        vec![
            Token::Uint(self.l1_batch_number.0.into()),
            Token::Bytes(encoded_data),
        ]
    }
}
