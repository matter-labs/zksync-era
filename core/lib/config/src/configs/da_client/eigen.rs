use serde::Deserialize;
use zksync_basic_types::secrets::PrivateKey;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub enum PointsSource {
    Path(String),
    Link(String),
}

impl Default for PointsSource {
    fn default() -> Self {
        PointsSource::Path("".to_string())
    }
}
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
    /// Maximun amount of time in milliseconds to wait for a status query response
    pub status_query_timeout: u64,
    /// Interval in milliseconds to query the status of a blob
    pub status_query_interval: u64,
    /// Wait for the blob to be finalized before returning the response
    pub wait_for_finalization: bool,
    /// Authenticated dispersal
    pub authenticated: bool,
    /// Verify the certificate of dispatched blobs
    pub verify_cert: bool,
    /// Path or link to the file containing the points used for KZG
    pub points_source: PointsSource,
    /// Chain ID of the Ethereum network
    pub chain_id: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EigenSecrets {
    pub private_key: PrivateKey,
}
