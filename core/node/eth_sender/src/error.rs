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

#[derive(Debug, thiserror::Error)]
pub enum TeeAggregatorError {
    #[error("Failed to retrieve TEE proofs for eth sender: {0}")]
    TeeProofRetrieval(#[from] zksync_dal::DalError),
    #[error("Failed to set eth_tx_id for TEE proof: {0}")]
    TeeProofEthTxUpdate(zksync_dal::DalError),
    #[error("Failed to retrieve pending DCAP collateral: {0}")]
    DcapCollateralRetrieval(zksync_dal::DalError),
    #[error("Failed to set eth_tx_id for DCAP field collateral: {0}")]
    DcapFieldCollateralUpdate(zksync_dal::DalError),
    #[error("Failed to set eth_tx_id for DCAP TCB info collateral: {0}")]
    DcapTcbInfoCollateralUpdate(zksync_dal::DalError),
    #[error("Failed to retrieve pending TEE attestations: {0}")]
    TeeAttestationRetrieval(zksync_dal::DalError),
    #[error("Failed to set eth_tx_id for TEE attestation: {0}")]
    TeeAttestationEthTxUpdate(zksync_dal::DalError),
}
