use anyhow::Context;
use zksync_basic_types::web3::contract::{Error, Tokenizable};

use crate::{ethabi::Token, H256};

#[derive(Debug, Clone)]
pub struct TransactionStatusCommitment {
    pub tx_hash: H256,
    pub status: bool,
}

impl Tokenizable for TransactionStatusCommitment {
    fn from_token(token: Token) -> Result<Self, Error> {
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
        .map_err(|err| Error::InvalidOutputType(format!("{err:#}")))
    }

    fn into_token(self) -> Token {
        Token::Tuple(vec![
            Token::FixedBytes(self.tx_hash.0.to_vec()),
            Token::Bool(self.status),
        ])
    }
}
