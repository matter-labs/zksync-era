use zksync_eth_client::{ContractCallError, EnrichedClientError};
use zksync_types::U256;

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
