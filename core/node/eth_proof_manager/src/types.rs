use zksync_eth_client::{ContractCallError, EnrichedClientError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProvingNetwork {
    None = 0,
    Fermah = 1,
    Lagrange = 2,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    ContractCallError(#[from] ContractCallError),
    ProviderError(#[from] EnrichedClientError),
    GenericError(#[from] anyhow::Error),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}