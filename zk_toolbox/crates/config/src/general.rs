use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{consts::GENERAL_FILE, traits::FileConfigWithDefaultName};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GeneralConfig {
    pub db: RocksDBConfig,
    pub eth: EthConfig,
    pub api: ApiConfig,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

impl FileConfigWithDefaultName for GeneralConfig {
    const FILE_NAME: &'static str = GENERAL_FILE;
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RocksDBConfig {
    pub state_keeper_db_path: PathBuf,
    pub merkle_tree: MerkleTreeDB,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MerkleTreeDB {
    pub path: PathBuf,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EthConfig {
    pub sender: EthSender,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EthSender {
    pub proof_sending_mode: String,
    pub pubdata_sending_mode: String,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ApiConfig {
    /// Configuration options for the Web3 JSON RPC servers.
    pub web3_json_rpc: Web3JsonRpcConfig,
    /// Configuration options for the Prometheus exporter.
    pub prometheus: PrometheusConfig,
    /// Configuration options for the Health check.
    pub healthcheck: HealthCheckConfig,
    /// Configuration options for Merkle tree API.
    pub merkle_tree: MerkleTreeApiConfig,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Web3JsonRpcConfig {
    /// Port to which the HTTP RPC server is listening.
    pub http_port: u16,
    /// URL to access HTTP RPC server.
    pub http_url: String,
    /// Port to which the WebSocket RPC server is listening.
    pub ws_port: u16,
    /// URL to access WebSocket RPC server.
    pub ws_url: String,
    /// Max possible limit of entities to be requested once.
    pub req_entities_limit: Option<u32>,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PrometheusConfig {
    /// Port to which the Prometheus exporter server is listening.
    pub listener_port: u16,
    /// URL of the push gateway.
    pub pushgateway_url: String,
    /// Push interval in ms.
    pub push_interval_ms: Option<u64>,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HealthCheckConfig {
    /// Port to which the REST server is listening.
    pub port: u16,
    /// Time limit in milliseconds to mark a health check as slow and log the corresponding warning.
    /// If not specified, the default value in the health check crate will be used.
    pub slow_time_limit_ms: Option<u64>,
    /// Time limit in milliseconds to abort a health check and return "not ready" status for the corresponding component.
    /// If not specified, the default value in the health check crate will be used.
    pub hard_time_limit_ms: Option<u64>,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

/// Configuration for the Merkle tree API.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MerkleTreeApiConfig {
    /// Port to bind the Merkle tree API server to.
    pub port: u16,
    #[serde(flatten)]
    pub other: serde_json::Value,
}
