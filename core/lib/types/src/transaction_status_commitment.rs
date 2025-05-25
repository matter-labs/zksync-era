use anyhow::Context;
use zksync_basic_types::{
    web3::contract::{Error, Tokenizable},
    U256,
};

use crate::{ethabi::Token, H256};

#[derive(Debug, Clone)]
pub struct TransactionStatusCommitment {
    pub tx_hash: H256,
    pub is_success: bool,
}

const PACKED_BYTES_SIZE: usize = 33;

impl TransactionStatusCommitment {
    pub fn get_packed_bytes(&self) -> [u8; PACKED_BYTES_SIZE] {
        let status_byte = if self.is_success { 1u8 } else { 0u8 };

        let mut data = [0u8; PACKED_BYTES_SIZE];
        data[0..32].copy_from_slice(self.tx_hash.as_bytes());
        data[32] = status_byte;

        data
    }

    // fn from_packed_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
    //     if bytes.len() != PACKED_BYTES_SIZE {
    //         anyhow::bail!("Wrong packed TransactionStatusCommitment length");
    //     }

    //     let status_byte = bytes[32];
    //     let is_success = match status_byte {
    //         0 => false,
    //         1 => true,
    //         _ => anyhow::bail!("Incorrect status bytee")
    //     };

    //     Ok(Self {
    //         tx_hash: H256::from_slice(&bytes[0..32]),
    //         is_success
    //     })
    // }
}

// impl Tokenizable for TransactionStatusCommitment {
//     fn from_token(token: Token) -> Result<Self, Error> {
//         (|| {
//             let as_bytes = token.into_bytes().context("not bytes array")?;

//             Self::from_packed_bytes(&as_bytes)
//         })()
//         .map_err(|err| Error::InvalidOutputType(format!("{err:#}")))
//     }

//     fn into_token(self) -> Token {
//         Token::Bytes(self.get_packed_bytes().to_vec())
//     }
// }
