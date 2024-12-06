use serde::Deserialize;
use zksync_basic_types::secrets::PrivateKey;
/// Configuration for the EigenDA remote disperser client.
#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
pub struct EigenConfig {
    /// URL of the Disperser RPC server
    pub disperser_rpc: String,
    /// Block height needed to reach in order to consider the blob finalized
    /// a value less or equal to 0 means that the disperser will not wait for finalization
    pub settlement_layer_confirmation_depth: i32,
    /// URL of the Ethereum RPC server
    pub eigenda_eth_rpc: String,
    /// Address of the service manager contract
    pub eigenda_svc_manager_address: String,
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

#[derive(Clone, Debug, PartialEq)]
pub struct EigenSecrets {
    pub private_key: PrivateKey,
}
