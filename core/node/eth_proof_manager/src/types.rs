use zksync_eth_client::{ContractCallError, EnrichedClientError};
use zksync_types::{ethabi::Token, web3::contract::Tokenize, H256, U256};

pub enum ProvingNetwork {
    None = 0,
    Fermah = 1,
    Lagrange = 2,
}

pub struct ProofRequest {
    pub id: H256,
    pub chain_id: U256,
    pub block_number: U256,
    pub proof_inputs_url: String,
    pub protocol_major: U256,
    pub protocol_minor: U256,
    pub protocol_patch: U256,
    pub timeout_after: U256,
    pub max_reward: U256,
    pub requested_reward: U256,
    pub proof: Vec<U256>,
}

pub struct ProofRequestProven {
    pub id: H256,
    pub chain_id: U256,
    pub block_number: U256,
    pub proof: Vec<U256>,
    pub assigned_to: ProvingNetwork,
}

pub struct ProofRequestParams {
    pub protocol_major: U256,
    pub protocol_minor: U256,
    pub protocol_patch: U256,
    pub proof_inputs_url: String,
    pub timeout_after: U256,
    pub max_reward: U256,
}

pub struct ProofRequestIdentifier {
    pub chain_id: U256,
    pub block_number: U256,
}

impl Tokenize for &ProofRequestIdentifier<'_> {
    fn into_tokens(self) -> Vec<Token> {
        vec![Token::Uint(self.chain_id), Token::Uint(self.block_number)]
    }
}

impl Tokenize for &ProofRequestParams<'_> {
    fn into_tokens(self) -> Vec<Token> {
        vec![
            Token::Uint(self.protocol_major),
            Token::Uint(self.protocol_minor),
            Token::Uint(self.protocol_patch),
            Token::String(self.proof_inputs_url),
            Token::Uint(self.timeout_after),
            Token::Uint(self.max_reward),
        ]
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    ContractCallError(#[from] ContractCallError),
    ProviderError(#[from] EnrichedClientError),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
