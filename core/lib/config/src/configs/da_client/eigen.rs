use serde::Deserialize;
use zksync_basic_types::secrets::PrivateKey;
/// Configuration for the EigenDA remote disperser client.
#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
pub struct EigenConfig {
    /// URL of the Disperser RPC server
    pub disperser_rpc: String,
    /// Block height needed to reach in order to consider the blob finalized
    /// a value less or equal to 0 means that the disperser will not wait for finalization
    pub eth_confirmation_depth: i32,
    /// URL of the Ethereum RPC server
    pub eigenda_eth_rpc: String,
    /// Address of the service manager contract
    pub eigenda_svc_manager_address: String,
    /// Maximum size permitted for a blob in bytes
    pub blob_size_limit: u32,
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
    /// Path to the file containing the points used for KZG
    pub path_to_points: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EigenSecrets {
    pub private_key: PrivateKey,
}
