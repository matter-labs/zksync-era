use serde::{Deserialize, Serialize};
use zksync_basic_types::{ethabi::Token, U256};

/// Encoded representation of the aggregated block proof.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EncodedAggregatedProof {
    pub aggregated_input: U256,
    pub proof: Vec<U256>,
    pub subproof_limbs: Vec<U256>,
    pub individual_vk_inputs: Vec<U256>,
    pub individual_vk_idxs: Vec<U256>,
}

impl EncodedAggregatedProof {
    pub fn get_eth_tx_args(&self) -> Token {
        let subproof_limbs = Token::Array(
            self.subproof_limbs
                .iter()
                .map(|v| Token::Uint(*v))
                .collect(),
        );
        let proof = Token::Array(
            self.proof
                .iter()
                .map(|p| Token::Uint(U256::from(p)))
                .collect(),
        );

        Token::Tuple(vec![subproof_limbs, proof])
    }
}

impl Default for EncodedAggregatedProof {
    fn default() -> Self {
        Self {
            aggregated_input: U256::default(),
            proof: vec![U256::default(); 34],
            subproof_limbs: vec![U256::default(); 16],
            individual_vk_inputs: vec![U256::default(); 1],
            individual_vk_idxs: vec![U256::default(); 1],
        }
    }
}

/// Encoded representation of the block proof.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EncodedSingleProof {
    pub inputs: Vec<U256>,
    pub proof: Vec<U256>,
}

impl Default for EncodedSingleProof {
    fn default() -> Self {
        Self {
            inputs: vec![U256::default(); 1],
            proof: vec![U256::default(); 33],
        }
    }
}
