use std::{
    collections::HashMap,
    fmt,
    net::SocketAddr,
    num::{NonZeroU32, NonZeroUsize},
    str::FromStr,
    time::Duration,
};

use anyhow::Context as _;
use serde::{de, Deserialize, Deserializer};
use zksync_basic_types::Address;

pub use crate::configs::PrometheusConfig;

/// API configuration.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ApiConfig {
    /// Configuration options for the Web3 JSON RPC servers.
    pub web3_json_rpc: Web3JsonRpcConfig,
    /// Configuration options for the Prometheus exporter.
    pub prometheus: PrometheusConfig,
    /// Configuration options for the Health check.
    pub healthcheck: HealthCheckConfig,
    /// Configuration options for Merkle tree API.
    pub merkle_tree: MerkleTreeApiConfig,
}

/// Response size limits for specific RPC methods.
///
/// The unit of measurement for contained limits depends on the context. In [`MaxResponseSize`],
/// limits are measured in bytes, but in configs, limits are specified in MiBs.
#[derive(Debug, Clone, PartialEq)]
pub struct MaxResponseSizeOverrides(HashMap<String, NonZeroUsize>);

impl<S: Into<String>> FromIterator<(S, NonZeroUsize)> for MaxResponseSizeOverrides {
    fn from_iter<I: IntoIterator<Item = (S, NonZeroUsize)>>(iter: I) -> Self {
        Self(
            iter.into_iter()
                .map(|(method_name, size)| (method_name.into(), size))
                .collect(),
        )
    }
}

impl FromStr for MaxResponseSizeOverrides {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut overrides = HashMap::new();
        for part in s.split(',') {
            let (method_name, size) = part.split_once('=').with_context(|| {
                format!("Part `{part}` doesn't have form <method_name>=<int>|None")
            })?;
            let method_name = method_name.trim();

            let size = size.trim();
            let size = if size == "None" {
                NonZeroUsize::MAX // No limit
            } else {
                size.parse().with_context(|| {
                    format!("`{size}` specified for method `{method_name}` is not a valid size")
                })?
            };

            if let Some(prev_size) = overrides.insert(method_name.to_owned(), size) {
                anyhow::bail!(
                    "Size override for `{method_name}` is redefined from {prev_size} to {size}"
                );
            }
        }
        Ok(Self(overrides))
    }
}

impl MaxResponseSizeOverrides {
    pub fn empty() -> Self {
        Self(HashMap::new())
    }

    /// Gets the override in bytes for the specified method, or `None` if it's not set.
    pub fn get(&self, method_name: &str) -> Option<usize> {
        self.0.get(method_name).copied().map(NonZeroUsize::get)
    }

    /// Iterates over all overrides.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, usize)> + '_ {
        self.0
            .iter()
            .map(|(method_name, size)| (method_name.as_str(), size.get()))
    }

    /// Scales the overrides by the specified scale factor, saturating them if applicable.
    pub fn scale(&self, factor: NonZeroUsize) -> Self {
        let scaled = self
            .0
            .iter()
            .map(|(method_name, &size)| (method_name.clone(), size.saturating_mul(factor)));
        Self(scaled.collect())
    }
}

impl<'de> Deserialize<'de> for MaxResponseSizeOverrides {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ParseVisitor;

        impl<'v> de::Visitor<'v> for ParseVisitor {
            type Value = MaxResponseSizeOverrides;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("comma-separated list of <method_name>=<size>|None tuples, such as: eth_call=2,zks_getProof=None")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                value.parse().map_err(E::custom)
            }
        }

        deserializer.deserialize_str(ParseVisitor)
    }
}

/// Response size limits for JSON-RPC servers.
#[derive(Debug)]
pub struct MaxResponseSize {
    /// Global limit applied to all RPC methods. Measured in bytes.
    pub global: usize,
    /// Limits applied to specific methods. Limits are measured in bytes; method names are full (e.g., `eth_call`).
    pub overrides: MaxResponseSizeOverrides,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Web3JsonRpcConfig {
    /// Port to which the HTTP RPC server is listening.
    pub http_port: u16,
    /// Port to which the WebSocket RPC server is listening.
    pub ws_port: u16,
    /// Max possible limit of entities to be requested once.
    pub req_entities_limit: Option<u32>,
    /// Whether to support HTTP methods that install filters and query filter changes.
    /// WS methods are unaffected.
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
    /// The factor by which to scale the gasLimit
    pub estimate_gas_scale_factor: f64,
    /// The max possible number of gas that `eth_estimateGas` is allowed to overestimate.
    pub estimate_gas_acceptable_overestimation: u32,
    /// Enables optimizations for the binary search of the gas limit in `eth_estimateGas`. These optimizations are currently
    /// considered experimental.
    #[serde(default)]
    pub estimate_gas_optimize_search: bool,
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
    /// Maximum lag in the number of blocks for the latest values cache after which the cache is reset. Greater values
    /// lead to increased the cache update latency, i.e., less storage queries being processed by the cache. OTOH, smaller values
    /// can lead to spurious resets when Postgres lags for whatever reason (e.g., when sealing L1 batches).
    pub latest_values_max_block_lag: Option<NonZeroU32>,
    /// Limit for fee history block range.
    pub fee_history_limit: Option<u64>,
    /// Maximum number of requests in a single batch JSON RPC request. Default is 500.
    pub max_batch_request_size: Option<usize>,
    /// Maximum response body size in MiBs. Default is 10 MiB.
    pub max_response_body_size_mb: Option<usize>,
    /// Method-specific overrides in MiBs for the maximum response body size.
    #[serde(default = "MaxResponseSizeOverrides::empty")]
    pub max_response_body_size_overrides_mb: MaxResponseSizeOverrides,
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
    /// List of L2 token addresses that are white-listed to use by paymasters
    /// (additionally to natively bridged tokens).
    #[serde(default)]
    pub whitelisted_tokens_for_aa: Vec<Address>,
    /// Enabled JSON RPC API namespaces. If not set, all namespaces will be available
    #[serde(default)]
    pub api_namespaces: Option<Vec<String>>,
    /// Enables extended tracing of RPC calls. This may negatively impact performance for nodes under high load
    /// (hundreds or thousands RPS).
    #[serde(default)]
    pub extended_api_tracing: bool,
}

impl Web3JsonRpcConfig {
    /// Creates a mock instance of `Web3JsonRpcConfig` to be used in tests.
    /// Ports and some fields that may affect execution are set to the same values used by default in
    /// the localhost environment. Other fields are set to default values.
    pub fn for_tests() -> Self {
        Self {
            http_port: 3050,
            ws_port: 3051,
            req_entities_limit: Some(10000),
            filters_disabled: false,
            filters_limit: Some(10000),
            subscriptions_limit: Some(10000),
            pubsub_polling_interval: Some(200),
            max_nonce_ahead: 50,
            gas_price_scale_factor: 1.2,
            estimate_gas_scale_factor: 1.5,
            estimate_gas_acceptable_overestimation: 1000,
            estimate_gas_optimize_search: false,
            max_tx_size: 1000000,
            vm_execution_cache_misses_limit: None,
            vm_concurrency_limit: None,
            factory_deps_cache_size_mb: None,
            initial_writes_cache_size_mb: None,
            latest_values_cache_size_mb: None,
            latest_values_max_block_lag: None,
            fee_history_limit: None,
            max_batch_request_size: None,
            max_response_body_size_mb: None,
            max_response_body_size_overrides_mb: MaxResponseSizeOverrides::empty(),
            websocket_requests_per_minute_limit: None,
            mempool_cache_update_interval: None,
            mempool_cache_size: None,
            tree_api_url: None,
            whitelisted_tokens_for_aa: vec![],
            api_namespaces: None,
            extended_api_tracing: false,
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

    /// Returns the maximum lag in the number of blocks for the latest values cache.
    pub fn latest_values_max_block_lag(&self) -> u32 {
        self.latest_values_max_block_lag.map_or(20, NonZeroU32::get)
    }

    pub fn fee_history_limit(&self) -> u64 {
        self.fee_history_limit.unwrap_or(1024)
    }

    pub fn max_batch_request_size(&self) -> usize {
        // The default limit is chosen to be reasonably permissive.
        self.max_batch_request_size.unwrap_or(500)
    }

    pub fn max_response_body_size(&self) -> MaxResponseSize {
        let scale = NonZeroUsize::new(super::BYTES_IN_MEGABYTE).unwrap();
        MaxResponseSize {
            global: self.max_response_body_size_mb.unwrap_or(10) * super::BYTES_IN_MEGABYTE,
            overrides: self.max_response_body_size_overrides_mb.scale(scale),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn working_with_max_response_size_overrides() {
        let overrides: MaxResponseSizeOverrides =
            "eth_call=1, eth_getTransactionReceipt=None,zks_getProof = 32 "
                .parse()
                .unwrap();
        assert_eq!(overrides.iter().len(), 3);
        assert_eq!(overrides.get("eth_call"), Some(1));
        assert_eq!(overrides.get("eth_getTransactionReceipt"), Some(usize::MAX));
        assert_eq!(overrides.get("zks_getProof"), Some(32));
        assert_eq!(overrides.get("eth_blockNumber"), None);

        let scaled = overrides.scale(NonZeroUsize::new(1_000).unwrap());
        assert_eq!(scaled.iter().len(), 3);
        assert_eq!(scaled.get("eth_call"), Some(1_000));
        assert_eq!(scaled.get("eth_getTransactionReceipt"), Some(usize::MAX));
        assert_eq!(scaled.get("zks_getProof"), Some(32_000));
        assert_eq!(scaled.get("eth_blockNumber"), None);
    }
}
