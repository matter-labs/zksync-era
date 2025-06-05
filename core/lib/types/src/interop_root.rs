use serde::{Deserialize, Serialize};

use crate::{ethabi::Token, u256_to_h256, U256};

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct InteropRoot {
    pub chain_id: u32,
    pub block_number: u32,
    pub sides: Vec<U256>, // The rolling hash of all the transactions in the miniblock
    pub received_timestamp: u64,
}

impl InteropRoot {
    pub fn new(chain_id: u32, block_number: u32, sides: Vec<U256>) -> Self {
        Self {
            chain_id,
            block_number,
            sides,
            received_timestamp: 0,
        }
    }

    pub fn into_token(self) -> Token {
        Token::Tuple(vec![
            Token::Uint(self.chain_id.into()),
            Token::Uint(self.block_number.into()),
            Token::Array(
                self.sides
                    .iter()
                    .map(|hash| Token::FixedBytes(u256_to_h256(*hash).as_bytes().to_vec()))
                    .collect(),
            ),
        ]) //
    }
}
