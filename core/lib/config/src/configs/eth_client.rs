// External uses
use serde::Deserialize;

/// Configuration for the Ethereum gateways.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ETHClientConfig {
    /// Numeric identifier of the L1 network (e.g. `9` for localhost).
    pub chain_id: u64,
    /// Address of the Ethereum node API.
    pub web3_url: String,
}
