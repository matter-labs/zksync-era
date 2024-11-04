use serde::Deserialize;
use zksync_basic_types::secrets::PrivateKey;

pub const EIGEN_MEMSTORE_CLIENT_NAME: &str = "MemStore";
pub const EIGEN_DISPERSER_CLIENT_NAME: &str = "Disperser";

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub enum EigenConfig {
    MemStore(MemStoreConfig),
    Disperser(DisperserConfig),
}

#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
pub struct MemStoreConfig {
    pub max_blob_size_bytes: u64,
    /// Blob expiration time in seconds
    pub blob_expiration: u64,
    /// Latency in milliseconds for get operations
    pub get_latency: u64,
    /// Latency in milliseconds for put operations
    pub put_latency: u64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
pub struct DisperserConfig {
    pub custom_quorum_numbers: Option<Vec<u32>>,
    pub disperser_rpc: String,
    pub eth_confirmation_depth: i32,
    pub eigenda_eth_rpc: String,
    pub eigenda_svc_manager_address: String,
    pub blob_size_limit: u64,
    pub status_query_timeout: u64,
    pub status_query_interval: u64,
    pub wait_for_finalization: bool,
    pub authenticated: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EigenSecrets {
    pub private_key: PrivateKey,
}
