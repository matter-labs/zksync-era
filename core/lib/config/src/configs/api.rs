use std::{net::SocketAddr, num::NonZeroU32, time::Duration};

use serde::Deserialize;
use zksync_basic_types::H256;

pub use crate::configs::PrometheusConfig;

/// API configuration.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ApiConfig {
    /// Configuration options for the Web3 JSON RPC servers.
    pub web3_json_rpc: Web3JsonRpcConfig,
    /// Configuration options for the REST servers.
    pub contract_verification: ContractVerificationApiConfig,
    /// Configuration options for the Prometheus exporter.
    pub prometheus: PrometheusConfig,
    /// Configuration options for the Health check.
    pub healthcheck: HealthCheckConfig,
    /// Configuration options for Merkle tree API.
    pub merkle_tree: MerkleTreeApiConfig,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
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
    /// Whether to support methods installing filters and querying filter changes.
    ///
    /// When to set this value to `true`:
    /// Filters are local to the specific node they were created at. Meaning if
    /// there are multiple nodes behind a load balancer the client cannot reliably
    /// query the previously created filter as the request might get routed to a
    /// different node.
    #[serde(default)]
    pub filters_disabled: bool,
    /// Max possible limit of filters to be in the state at once.
    pub filters_limit: Option<u32>,
    /// Max possible limit of subscriptions to be in the state at once.
    pub subscriptions_limit: Option<u32>,
    /// Interval between polling db for pubsub (in ms).
    pub pubsub_polling_interval: Option<u64>,
    /// Tx nonce: how far ahead from the committed nonce can it be.
    pub max_nonce_ahead: u32,
    /// The multiplier to use when suggesting gas price. Should be higher than one,
    /// otherwise if the L1 prices soar, the suggested gas price won't be sufficient to be included in block
    pub gas_price_scale_factor: f64,
    /// Timeout for requests (in s)
    pub request_timeout: Option<u64>,
    /// Private keys for accounts managed by node
    pub account_pks: Option<Vec<H256>>,
    /// The factor by which to scale the gasLimit
    pub estimate_gas_scale_factor: f64,
    /// The max possible number of gas that `eth_estimateGas` is allowed to overestimate.
    pub estimate_gas_acceptable_overestimation: u32,
    /// Whether to use the compatibility mode for gas estimation for L1->L2 transactions.
    /// During the migration to the 1.4.1 fee model, there will be a period, when the server
    /// will already have the 1.4.1 fee model, while the L1 contracts will still expect the transactions
    /// to use the previous fee model with much higher overhead.
    ///
    /// When set to `true`, the API will ensure to return gasLimit is high enough overhead for both the old
    /// and the new fee model when estimating L1->L2 transactions.  
    pub l1_to_l2_transactions_compatibility_mode: bool,
    ///  Max possible size of an ABI encoded tx (in bytes).
    pub max_tx_size: usize,
    /// Max number of cache misses during one VM execution. If the number of cache misses exceeds this value, the API server panics.
    /// This is a temporary solution to mitigate API request resulting in thousands of DB queries.
    pub vm_execution_cache_misses_limit: Option<usize>,
    /// Max number of VM instances to be concurrently spawned by the API server.
    /// This option can be tweaked down if the API server is running out of memory.
    /// If not set, the VM concurrency limit will be efficiently disabled.
    pub vm_concurrency_limit: Option<usize>,
    /// Smart contract cache size in MiBs. The default value is 128 MiB.
    pub factory_deps_cache_size_mb: Option<usize>,
    /// Initial writes cache size in MiBs. The default value is 32 MiB.
    pub initial_writes_cache_size_mb: Option<usize>,
    /// Latest values cache size in MiBs. The default value is 128 MiB. If set to 0, the latest
    /// values cache will be disabled.
    pub latest_values_cache_size_mb: Option<usize>,
    /// Limit for fee history block range.
    pub fee_history_limit: Option<u64>,
    /// Maximum number of requests in a single batch JSON RPC request. Default is 500.
    pub max_batch_request_size: Option<usize>,
    /// Maximum response body size in MiBs. Default is 10 MiB.
    pub max_response_body_size_mb: Option<usize>,
    /// Maximum number of requests per minute for the WebSocket server.
    /// The value is per active connection.
    /// Note: For HTTP, rate limiting is expected to be configured on the infra level.
    pub websocket_requests_per_minute_limit: Option<NonZeroU32>,
    /// Tree API url, currently used to proxy `getProof` calls to the tree
    pub tree_api_url: Option<String>,
    /// Polling period for mempool cache update - how often the mempool cache is updated from the database.
    /// In milliseconds. Default is 50 milliseconds.
    pub mempool_cache_update_interval: Option<u64>,
    /// Maximum number of transactions to be stored in the mempool cache. Default is 10000.
    pub mempool_cache_size: Option<usize>,
}

impl Web3JsonRpcConfig {
    /// Creates a mock instance of `Web3JsonRpcConfig` to be used in tests.
    /// Ports and some fields that may affect execution are set to the same values used by default in
    /// the localhost environment. Other fields are set to default values.
    pub fn for_tests() -> Self {
        Self {
            http_port: 3050,
            http_url: "http://localhost:3050".into(),
            ws_port: 3051,
            ws_url: "ws://localhost:3051".into(),
            req_entities_limit: Some(10000),
            filters_disabled: false,
            filters_limit: Some(10000),
            subscriptions_limit: Some(10000),
            pubsub_polling_interval: Some(200),
            max_nonce_ahead: 50,
            gas_price_scale_factor: 1.2,
            request_timeout: Default::default(),
            account_pks: Default::default(),
            estimate_gas_scale_factor: 1.2,
            estimate_gas_acceptable_overestimation: 1000,
            l1_to_l2_transactions_compatibility_mode: true,
            max_tx_size: 1000000,
            vm_execution_cache_misses_limit: Default::default(),
            vm_concurrency_limit: Default::default(),
            factory_deps_cache_size_mb: Default::default(),
            initial_writes_cache_size_mb: Default::default(),
            latest_values_cache_size_mb: Default::default(),
            fee_history_limit: Default::default(),
            max_batch_request_size: Default::default(),
            max_response_body_size_mb: Default::default(),
            websocket_requests_per_minute_limit: Default::default(),
            mempool_cache_update_interval: Default::default(),
            mempool_cache_size: Default::default(),
            tree_api_url: None,
        }
    }

    pub fn http_bind_addr(&self) -> SocketAddr {
        SocketAddr::new("0.0.0.0".parse().unwrap(), self.http_port)
    }

    pub fn ws_bind_addr(&self) -> SocketAddr {
        SocketAddr::new("0.0.0.0".parse().unwrap(), self.ws_port)
    }

    pub fn req_entities_limit(&self) -> usize {
        self.req_entities_limit.unwrap_or_else(|| 2u32.pow(10)) as usize
    }

    pub fn filters_limit(&self) -> usize {
        self.filters_limit.unwrap_or(10000) as usize
    }

    pub fn subscriptions_limit(&self) -> usize {
        self.subscriptions_limit.unwrap_or(10000) as usize
    }

    pub fn pubsub_interval(&self) -> Duration {
        Duration::from_millis(self.pubsub_polling_interval.unwrap_or(200))
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout.unwrap_or(10))
    }

    pub fn account_pks(&self) -> Vec<H256> {
        self.account_pks.clone().unwrap_or_default()
    }

    pub fn vm_concurrency_limit(&self) -> usize {
        // The default limit is large so that it does not create a bottleneck on its own.
        // VM execution can still be limited by Tokio runtime parallelism and/or the number
        // of DB connections in a pool.
        self.vm_concurrency_limit.unwrap_or(2_048)
    }

    /// Returns the size of factory dependencies cache in bytes.
    pub fn factory_deps_cache_size(&self) -> usize {
        self.factory_deps_cache_size_mb.unwrap_or(128) * super::BYTES_IN_MEGABYTE
    }

    /// Returns the size of initial writes cache in bytes.
    pub fn initial_writes_cache_size(&self) -> usize {
        self.initial_writes_cache_size_mb.unwrap_or(32) * super::BYTES_IN_MEGABYTE
    }

    /// Returns the size of latest values cache in bytes.
    pub fn latest_values_cache_size(&self) -> usize {
        self.latest_values_cache_size_mb.unwrap_or(128) * super::BYTES_IN_MEGABYTE
    }

    pub fn fee_history_limit(&self) -> u64 {
        self.fee_history_limit.unwrap_or(1024)
    }

    pub fn max_batch_request_size(&self) -> usize {
        // The default limit is chosen to be reasonably permissive.
        self.max_batch_request_size.unwrap_or(500)
    }

    pub fn max_response_body_size(&self) -> usize {
        self.max_response_body_size_mb.unwrap_or(10) * super::BYTES_IN_MEGABYTE
    }

    pub fn websocket_requests_per_minute_limit(&self) -> NonZeroU32 {
        // The default limit is chosen to be reasonably permissive.
        self.websocket_requests_per_minute_limit
            .unwrap_or(NonZeroU32::new(6000).unwrap())
    }

    pub fn tree_api_url(&self) -> Option<&str> {
        self.tree_api_url.as_deref()
    }

    pub fn mempool_cache_update_interval(&self) -> Duration {
        Duration::from_millis(self.mempool_cache_update_interval.unwrap_or(50))
    }

    pub fn mempool_cache_size(&self) -> usize {
        self.mempool_cache_size.unwrap_or(10_000)
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct HealthCheckConfig {
    /// Port to which the REST server is listening.
    pub port: u16,
    /// Time limit in milliseconds to mark a health check as slow and log the corresponding warning.
    /// If not specified, the default value in the health check crate will be used.
    pub slow_time_limit_ms: Option<u64>,
    /// Time limit in milliseconds to abort a health check and return "not ready" status for the corresponding component.
    /// If not specified, the default value in the health check crate will be used.
    pub hard_time_limit_ms: Option<u64>,
}

impl HealthCheckConfig {
    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new("0.0.0.0".parse().unwrap(), self.port)
    }

    pub fn slow_time_limit(&self) -> Option<Duration> {
        self.slow_time_limit_ms.map(Duration::from_millis)
    }

    pub fn hard_time_limit(&self) -> Option<Duration> {
        self.hard_time_limit_ms.map(Duration::from_millis)
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ContractVerificationApiConfig {
    /// Port to which the REST server is listening.
    pub port: u16,
    /// URL to access REST server.
    pub url: String,
}

impl ContractVerificationApiConfig {
    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new("0.0.0.0".parse().unwrap(), self.port)
    }
}

/// Configuration for the Merkle tree API.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MerkleTreeApiConfig {
    /// Port to bind the Merkle tree API server to.
    #[serde(default = "MerkleTreeApiConfig::default_port")]
    pub port: u16,
}

impl MerkleTreeApiConfig {
    const fn default_port() -> u16 {
        3_072
    }
}
