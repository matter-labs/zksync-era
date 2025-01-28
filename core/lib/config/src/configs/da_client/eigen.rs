use std::str::FromStr;

use serde::Deserialize;
use zksync_basic_types::{secrets::PrivateKey, url::SensitiveUrl, Address, H160};

/// Default address of the EigenDA service manager contract deployed on Holesky.
const DEFAULT_EIGENDA_SVC_MANAGER_ADDRESS: H160 = H160([
    0xd4, 0xa7, 0xe1, 0xbd, 0x80, 0x15, 0x05, 0x72, 0x93, 0xf0, 0xd0, 0xa5, 0x57, 0x08, 0x8c, 0x28,
    0x69, 0x42, 0xe8, 0x4b,
]);

/// Configuration for the EigenDA remote disperser client.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct EigenConfig {
    /// URL of the Disperser RPC server
    pub disperser_rpc: String,
    /// Block height needed to reach in order to consider the blob finalized
    /// a value less or equal to 0 means that the disperser will not wait for finalization
    pub settlement_layer_confirmation_depth: u32,
    /// URL of the Ethereum RPC server
    pub eigenda_eth_rpc: Option<SensitiveUrl>,
    /// Address of the service manager contract
    pub eigenda_svc_manager_address: Address,
    /// Wait for the blob to be finalized before returning the response
    pub wait_for_finalization: bool,
    /// Authenticated dispersal
    pub authenticated: bool,
    /// Optional path to downloaded points directory
    pub points_dir: Option<String>,
    /// Url to the file containing the G1 point used for KZG
    pub g1_url: String,
    /// Url to the file containing the G2 point used for KZG
    pub g2_url: String,
}

impl Default for EigenConfig {
    fn default() -> Self {
        Self {
            disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
            settlement_layer_confirmation_depth: 0,
            eigenda_eth_rpc: Some(SensitiveUrl::from_str("https://ethereum-holesky-rpc.publicnode.com").unwrap()), // Safe to unwrap, never fails
            eigenda_svc_manager_address: DEFAULT_EIGENDA_SVC_MANAGER_ADDRESS,
            wait_for_finalization: false,
            authenticated: false,
            points_dir: None,
            g1_url: "https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g1.point".to_string(),
            g2_url: "https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g2.point.powerOf2".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EigenSecrets {
    pub private_key: PrivateKey,
}
