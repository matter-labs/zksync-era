use serde::{Deserialize, Serialize};

use crate::{ethabi::Token, L2ChainId, H256};

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct InteropRoot {
    pub chain_id: L2ChainId,
    pub block_number: u32,
    pub sides: Vec<H256>, // For proof based interop, the sides contain a single value: the messageRoot
    pub received_timestamp: u64,
}

impl InteropRoot {
    pub fn new(chain_id: L2ChainId, block_number: u32, sides: Vec<H256>) -> Self {
        Self {
            chain_id,
            block_number,
            sides,
            received_timestamp: 0,
        }
    }

    pub fn into_token(self) -> Token {
        Token::Tuple(vec![
            Token::Uint(self.chain_id.as_u64().into()),
            Token::Uint(self.block_number.into()),
            Token::Array(
                self.sides
                    .iter()
                    .map(|hash| Token::FixedBytes(hash.as_bytes().to_vec()))
                    .collect(),
            ),
        ])
    }
}
