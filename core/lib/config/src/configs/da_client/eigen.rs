use std::str::FromStr;

use serde::Deserialize;
use zksync_basic_types::{secrets::PrivateKey, Address, H160};

/// Default address of the EigenDA service manager contract deployed on Holesky.
const DEFAULT_EIGENDA_SVC_MANAGER_ADDRESS: &str = "0xD4A7E1Bd8015057293f0D0A557088c286942e84b";

/// Configuration for the EigenDA remote disperser client.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct EigenConfig {
    /// URL of the Disperser RPC server
    pub disperser_rpc: String,
    /// Block height needed to reach in order to consider the blob finalized
    /// a value less or equal to 0 means that the disperser will not wait for finalization
    pub settlement_layer_confirmation_depth: u32,
    /// URL of the Ethereum RPC server
    pub eigenda_eth_rpc: Option<String>,
    /// Address of the service manager contract
    pub eigenda_svc_manager_address: Address,
    /// Wait for the blob to be finalized before returning the response
    pub wait_for_finalization: bool,
    /// Authenticated dispersal
    pub authenticated: bool,
    /// Url to the file containing the G1 point used for KZG
    pub g1_url: String,
    /// Url to the file containing the G2 point used for KZG
    pub g2_url: String,
    /// Chain ID of the Ethereum network
    pub chain_id: u64,
}

impl Default for EigenConfig {
    fn default() -> Self {
        Self {
            disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
            settlement_layer_confirmation_depth: 0,
            eigenda_eth_rpc: Some("https://ethereum-holesky-rpc.publicnode.com".to_string()),
            eigenda_svc_manager_address: H160::from_str(DEFAULT_EIGENDA_SVC_MANAGER_ADDRESS).unwrap_or_default(),
            wait_for_finalization: false,
            authenticated: false,
            g1_url: "https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g1.point".to_string(),
            g2_url: "https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g2.point.powerOf2".to_string(),
            chain_id: 19000,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EigenSecrets {
    pub private_key: PrivateKey,
}
