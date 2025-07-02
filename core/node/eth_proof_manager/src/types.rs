use zksync_eth_client::{ContractCallError, EnrichedClientError, SigningError};
use zksync_types::{ethabi, U256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProvingNetwork {
    None = 0,
    Fermah = 1,
    Lagrange = 2,
}

impl ProvingNetwork {
    pub fn from_u256(u: U256) -> Self {
        match u.as_u32() {
            0 => Self::None,
            1 => Self::Fermah,
            2 => Self::Lagrange,
            _ => panic!("Invalid proving network: {}", u),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProofRequestIdentifier {
    pub chain_id: u64,     // uint256
    pub block_number: u64, // uint256
}

impl ProofRequestIdentifier {
    pub fn into_tokens(self) -> ethabi::Token {
        ethabi::Token::Tuple(vec![
            ethabi::Token::Uint(U256::from(self.chain_id)),
            ethabi::Token::Uint(U256::from(self.block_number)),
        ])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProofRequestParams {
    pub protocol_major: u32,      // uint32
    pub protocol_minor: u32,      // uint32
    pub protocol_patch: u32,      // uint32
    pub proof_inputs_url: String, // string
    pub timeout_after: u64,       // uint256
    pub max_reward: u64,          // uint256
}

impl ProofRequestParams {
    pub fn into_tokens(self) -> ethabi::Token {
        ethabi::Token::Tuple(vec![
            ethabi::Token::Uint(U256::from(self.protocol_major)),
            ethabi::Token::Uint(U256::from(self.protocol_minor)),
            ethabi::Token::Uint(U256::from(self.protocol_patch)),
            ethabi::Token::String(self.proof_inputs_url),
            ethabi::Token::Uint(U256::from(self.timeout_after)),
            ethabi::Token::Uint(U256::from(self.max_reward)),
        ])
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    ContractCallError(#[from] ContractCallError),
    EthabiError(#[from] ethabi::Error),
    SigningError(#[from] SigningError),
    ProviderError(#[from] EnrichedClientError),
    GenericError(#[from] anyhow::Error),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
