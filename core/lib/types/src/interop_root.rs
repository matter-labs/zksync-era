use serde::{Deserialize, Serialize};

use crate::{ethabi::Token, u256_to_h256, U256};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct InteropRoot {
    pub chain_id: u32,
    pub block_number: u32,
    pub sides: Vec<U256>, // The rolling hash of all the transactions in the miniblock
}

impl InteropRoot {
    pub fn new(chain_id: u32, block_number: u32, sides: Vec<U256>) -> Self {
        Self {
            chain_id,
            block_number,
            sides,
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

    pub fn dummy_interop_root() -> Self {
        Self {
            chain_id: 54321,
            block_number: 4294967295,
            sides: vec![U256::from_dec_str("111222").unwrap()],
        }
    }
}

impl Clone for InteropRoot {
    fn clone(&self) -> Self {
        Self {
            chain_id: self.chain_id,
            block_number: self.block_number,
            sides: self.sides.clone(),
        }
    }
}
