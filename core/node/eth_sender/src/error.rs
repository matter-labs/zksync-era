use zksync_eth_client::{ContractCallError, EnrichedClientError};
use zksync_types::web3::contract;

#[derive(Debug, thiserror::Error)]
pub enum EthSenderError {
    #[error("Ethereum gateway error: {0}")]
    EthereumGateway(#[from] EnrichedClientError),
    #[error("Contract call error: {0}")]
    ContractCall(#[from] ContractCallError),
    #[error("Token parsing error: {0}")]
    Parse(#[from] contract::Error),
    #[error("Max base fee exceeded")]
    ExceedMaxBaseFee,
}

impl EthSenderError {
    pub fn is_retriable(&self) -> bool {
        match self {
            EthSenderError::EthereumGateway(err) => err.is_retryable(),
            _ => false,
        }
    }
}
