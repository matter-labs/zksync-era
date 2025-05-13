use serde::Deserialize;
use zksync_basic_types::{secrets::PrivateKey, url::SensitiveUrl, Address};

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub enum PointsSource {
    Path(String),
    /// g1_url, g2_url
    Url((String, String)),
}

/// Configuration for the EigenDA remote disperser client.
///
/// This configuration is meant to be used by the EigenDA V1 client.
/// The M0 stands for Milestone 0, an insecure integration, where the dispersal is not verified.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct EigenConfigV1M0 {
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
    /// Points source
    pub points_source: PointsSource,
    /// Custom quorum numbers
    pub custom_quorum_numbers: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EigenSecretsV1M0 {
    pub private_key: PrivateKey,
}
