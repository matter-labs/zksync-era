use std::{
    collections::HashMap,
    net::SocketAddr,
    num::{NonZeroU32, NonZeroUsize},
    str::FromStr,
    time::Duration,
};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use smart_config::{
    de::{Delimited, OrString, Serde, WellKnown},
    metadata::{SizeUnit, TimeUnit},
    ByteSize, DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::Address;

/// API configuration.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct ApiConfig {
    /// Configuration options for the Web3 JSON RPC servers.
    #[config(nest)]
    pub web3_json_rpc: Web3JsonRpcConfig,
    /// Configuration options for the Health check.
    #[config(nest)]
    pub healthcheck: HealthCheckConfig,
    /// Configuration options for Merkle tree API.
    #[config(nest)]
    pub merkle_tree: MerkleTreeApiConfig,
}

/// Response size limits for specific RPC methods.
///
/// The unit of measurement for contained limits depends on the context. In [`MaxResponseSize`],
/// limits are measured in bytes, but in configs, limits are specified in MiBs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MaxResponseSizeOverrides(HashMap<String, Option<NonZeroUsize>>);

impl<S: Into<String>> FromIterator<(S, Option<NonZeroUsize>)> for MaxResponseSizeOverrides {
    fn from_iter<I: IntoIterator<Item = (S, Option<NonZeroUsize>)>>(iter: I) -> Self {
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
                None
            } else {
                Some(size.parse().with_context(|| {
                    format!("`{size}` specified for method `{method_name}` is not a valid size")
                })?)
            };

            if let Some(prev_size) = overrides.insert(method_name.to_owned(), size) {
                anyhow::bail!(
                    "Size override for `{method_name}` is redefined from {prev_size:?} to {size:?}"
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
        self.0
            .get(method_name)
            .copied()
            .map(|size| size.map_or(usize::MAX, NonZeroUsize::get))
    }

    /// Iterates over all overrides.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, usize)> + '_ {
        self.0.iter().map(|(method_name, size)| {
            (
                method_name.as_str(),
                size.map_or(usize::MAX, NonZeroUsize::get),
            )
        })
    }

    /// Scales the overrides by the specified scale factor, saturating them if applicable.
    pub fn scale(&self, factor: NonZeroUsize) -> Self {
        let scaled = self.0.iter().map(|(method_name, &size)| {
            (
                method_name.clone(),
                size.map(|size| size.saturating_mul(factor)),
            )
        });
        Self(scaled.collect())
    }
}

impl WellKnown for MaxResponseSizeOverrides {
    type Deserializer = OrString<Serde![object]>;
    const DE: Self::Deserializer = OrString(Serde![object]);
}

/// Response size limits for JSON-RPC servers.
#[derive(Debug)]
pub struct MaxResponseSize {
    /// Global limit applied to all RPC methods. Measured in bytes.
    pub global: usize,
    /// Limits applied to specific methods. Limits are measured in bytes; method names are full (e.g., `eth_call`).
    pub overrides: MaxResponseSizeOverrides,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct Web3JsonRpcConfig {
    /// Port to which the HTTP RPC server is listening.
    #[config(default_t = 3_050)]
    pub http_port: u16,
    /// Port to which the WebSocket RPC server is listening.
    #[config(default_t = 3_051)]
    pub ws_port: u16,
    /// Max possible limit of entities to be requested once.
    #[config(default_t = 1_024)]
    pub req_entities_limit: u32,
    /// Whether to support HTTP methods that install filters and query filter changes.
    /// WS methods are unaffected.
    ///
    /// When to set this value to `true`:
    /// Filters are local to the specific node they were created at. Meaning if
    /// there are multiple nodes behind a load balancer the client cannot reliably
    /// query the previously created filter as the request might get routed to a
    /// different node.
    #[config(default)]
    pub filters_disabled: bool,
    /// Max possible limit of filters to be in the state at once.
    #[config(default_t = 10_000)]
    pub filters_limit: usize,
    /// Max possible limit of subscriptions to be in the state at once.
    #[config(default_t = 10_000)]
    pub subscriptions_limit: usize,
    /// Interval between polling db for pubsub (in ms).
    #[config(default_t = Duration::from_millis(200), with = ((), TimeUnit::Millis))]
    pub pubsub_polling_interval: Duration,
    /// Tx nonce: how far ahead from the committed nonce can it be.
    #[config(default_t = 50)]
    pub max_nonce_ahead: u32,
    /// The multiplier to use when suggesting gas price. Should be higher than one,
    /// otherwise if the L1 prices soar, the suggested gas price won't be sufficient to be included in block
    #[config(default_t = 1.5)]
    pub gas_price_scale_factor: f64,
    /// The factor by which to scale the gasLimit
    #[config(default_t = 1.3)]
    pub estimate_gas_scale_factor: f64,
    /// The max possible number of gas that `eth_estimateGas` is allowed to overestimate.
    #[config(default_t = 1_000)]
    pub estimate_gas_acceptable_overestimation: u32,
    /// Enables optimizations for the binary search of the gas limit in `eth_estimateGas`. These optimizations are currently
    /// considered experimental.
    #[config(default)]
    pub estimate_gas_optimize_search: bool,
    ///  Max possible size of an ABI encoded tx (in bytes).
    #[config(default_t = 10 * 1_024 * 1_024)]
    pub max_tx_size: usize,
    /// Max number of cache misses during one VM execution. If the number of cache misses exceeds this value, the API server panics.
    /// This is a temporary solution to mitigate API request resulting in thousands of DB queries.
    pub vm_execution_cache_misses_limit: Option<usize>,
    /// Max number of VM instances to be concurrently spawned by the API server.
    /// This option can be tweaked down if the API server is running out of memory.
    /// If not set, the VM concurrency limit will be efficiently disabled.
    #[config(default_t = 2_048)]
    pub vm_concurrency_limit: usize,
    /// Smart contract cache size in MiBs. The default value is 128 MiB.
    #[config(with = SizeUnit::MiB, default_t = ByteSize::new(128, SizeUnit::MiB))]
    pub factory_deps_cache_size_mb: ByteSize,
    /// Initial writes cache size in MiBs. The default value is 32 MiB.
    #[config(with = SizeUnit::MiB, default_t = ByteSize::new(32, SizeUnit::MiB))]
    pub initial_writes_cache_size_mb: ByteSize,
    /// Latest values cache size in MiBs. The default value is 128 MiB. If set to 0, the latest
    /// values cache will be disabled.
    #[config(with = SizeUnit::MiB, default_t = ByteSize::new(128, SizeUnit::MiB))]
    pub latest_values_cache_size_mb: ByteSize,
    /// Maximum lag in the number of blocks for the latest values cache after which the cache is reset. Greater values
    /// lead to increased the cache update latency, i.e., less storage queries being processed by the cache. OTOH, smaller values
    /// can lead to spurious resets when Postgres lags for whatever reason (e.g., when sealing L1 batches).
    #[config(default_t = NonZeroU32::new(20).unwrap())]
    pub latest_values_max_block_lag: NonZeroU32,
    /// Limit for fee history block range.
    #[config(default_t = 1_024)]
    pub fee_history_limit: u64,
    /// Maximum number of requests in a single batch JSON RPC request. Default is 500.
    #[config(default_t = 500)]
    pub max_batch_request_size: usize,
    /// Maximum response body size in MiBs. Default is 10 MiB.
    #[config(with = SizeUnit::MiB, default_t = ByteSize::new(10, SizeUnit::MiB))]
    pub max_response_body_size_mb: ByteSize,
    /// Method-specific overrides in MiBs for the maximum response body size.
    #[config(default = MaxResponseSizeOverrides::empty)]
    pub max_response_body_size_overrides_mb: MaxResponseSizeOverrides,
    /// Maximum number of requests per minute for the WebSocket server.
    /// The value is per active connection.
    /// Note: For HTTP, rate limiting is expected to be configured on the infra level.
    #[config(default_t = NonZeroU32::new(6_000).unwrap())]
    pub websocket_requests_per_minute_limit: NonZeroU32,
    /// Tree API url, currently used to proxy `getProof` calls to the tree
    pub tree_api_url: Option<String>,
    /// Polling period for mempool cache update - how often the mempool cache is updated from the database.
    /// In milliseconds. Default is 50 milliseconds.
    #[config(default_t = Duration::from_millis(50), with = ((), TimeUnit::Millis))]
    pub mempool_cache_update_interval: Duration,
    /// Maximum number of transactions to be stored in the mempool cache. Default is 10000.
    #[config(default_t = 10_000)]
    pub mempool_cache_size: usize,
    /// List of L2 token addresses that are white-listed to use by paymasters
    /// (additionally to natively bridged tokens).
    #[config(default, with = Delimited(","))]
    pub whitelisted_tokens_for_aa: Vec<Address>,
    /// Enabled JSON RPC API namespaces. If not set, all namespaces will be available
    #[config(with = Delimited(","))]
    pub api_namespaces: Option<Vec<String>>,
    /// Enables extended tracing of RPC calls. This may negatively impact performance for nodes under high load
    /// (hundreds or thousands RPS).
    #[config(default)]
    pub extended_api_tracing: bool,
}

impl Web3JsonRpcConfig {
    /// Creates a mock instance of `Web3JsonRpcConfig` to be used in tests.
    /// Ports and some fields that may affect execution are set to the same values used by default in
    /// the localhost environment. Other fields are set to default values.
    pub fn for_tests() -> Self {
        Self {
            gas_price_scale_factor: 1.2,
            estimate_gas_scale_factor: 1.5,
            ..Self::default()
        }
    }

    pub fn http_bind_addr(&self) -> SocketAddr {
        SocketAddr::new("0.0.0.0".parse().unwrap(), self.http_port)
    }

    pub fn ws_bind_addr(&self) -> SocketAddr {
        SocketAddr::new("0.0.0.0".parse().unwrap(), self.ws_port)
    }

    pub fn max_response_body_size(&self) -> MaxResponseSize {
        let scale = NonZeroUsize::new(super::BYTES_IN_MEGABYTE).unwrap();
        MaxResponseSize {
            global: self.max_response_body_size_mb.0 as usize,
            overrides: self.max_response_body_size_overrides_mb.scale(scale),
        }
    }
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct HealthCheckConfig {
    /// Port to which the REST server is listening.
    pub port: u16,
    /// Time limit in milliseconds to mark a health check as slow and log the corresponding warning.
    /// If not specified, the default value in the health check crate will be used.
    pub slow_time_limit: Option<Duration>,
    /// Time limit in milliseconds to abort a health check and return "not ready" status for the corresponding component.
    /// If not specified, the default value in the health check crate will be used.
    pub hard_time_limit: Option<Duration>,
}

impl HealthCheckConfig {
    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new("0.0.0.0".parse().unwrap(), self.port)
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
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct MerkleTreeApiConfig {
    /// Port to bind the Merkle tree API server to.
    #[config(default_t = 3_072)]
    pub port: u16,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Environment, Yaml};

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

    fn expected_config() -> ApiConfig {
        ApiConfig {
            web3_json_rpc: Web3JsonRpcConfig {
                http_port: 3050,
                ws_port: 3051,
                req_entities_limit: 10000,
                filters_disabled: false,
                filters_limit: 10000,
                subscriptions_limit: 10000,
                pubsub_polling_interval: Duration::from_millis(200),
                max_nonce_ahead: 5,
                estimate_gas_scale_factor: 1.0f64,
                gas_price_scale_factor: 1.2,
                estimate_gas_acceptable_overestimation: 1000,
                estimate_gas_optimize_search: true,
                max_tx_size: 1000000,
                vm_execution_cache_misses_limit: Some(1000),
                vm_concurrency_limit: 512,
                factory_deps_cache_size_mb: ByteSize::new(128, SizeUnit::MiB),
                initial_writes_cache_size_mb: ByteSize::new(32, SizeUnit::MiB),
                latest_values_cache_size_mb: ByteSize::new(256, SizeUnit::MiB),
                latest_values_max_block_lag: NonZeroU32::new(50).unwrap(),
                fee_history_limit: 100,
                max_batch_request_size: 200,
                max_response_body_size_mb: ByteSize::new(10, SizeUnit::MiB),
                max_response_body_size_overrides_mb: [
                    ("eth_call", NonZeroUsize::new(1)),
                    ("eth_getTransactionReceipt", None),
                    ("zks_getProof", NonZeroUsize::new(32)),
                ]
                .into_iter()
                .collect(),
                websocket_requests_per_minute_limit: NonZeroU32::new(10).unwrap(),
                tree_api_url: Some("http://tree/".into()),
                mempool_cache_update_interval: Duration::from_millis(50),
                mempool_cache_size: 10000,
                whitelisted_tokens_for_aa: vec![
                    Address::from_low_u64_be(1),
                    Address::from_low_u64_be(2),
                ],
                api_namespaces: Some(vec!["debug".to_string()]),
                extended_api_tracing: true,
            },
            healthcheck: HealthCheckConfig {
                port: 8081,
                slow_time_limit: Some(Duration::from_millis(250)),
                hard_time_limit: Some(Duration::from_millis(2_000)),
            },
            merkle_tree: MerkleTreeApiConfig { port: 8082 },
        }
    }

    #[test]
    fn parsing_api_config() {
        let env = r#"
            API_WEB3_JSON_RPC_HTTP_PORT="3050"
            API_WEB3_JSON_RPC_HTTP_URL="http://127.0.0.1:3050"
            API_WEB3_JSON_RPC_WS_PORT="3051"
            API_WEB3_JSON_RPC_WS_URL="ws://127.0.0.1:3051"
            API_WEB3_JSON_RPC_REQ_ENTITIES_LIMIT=10000
            API_WEB3_JSON_RPC_FILTERS_DISABLED=false
            API_WEB3_JSON_RPC_FILTERS_LIMIT=10000
            API_WEB3_JSON_RPC_SUBSCRIPTIONS_LIMIT=10000
            API_WEB3_JSON_RPC_PUBSUB_POLLING_INTERVAL=200
            API_WEB3_JSON_RPC_MAX_NONCE_AHEAD=5
            API_WEB3_JSON_RPC_GAS_PRICE_SCALE_FACTOR=1.2
            API_WEB3_JSON_RPC_ESTIMATE_GAS_OPTIMIZE_SEARCH=true
            API_WEB3_JSON_RPC_VM_EXECUTION_CACHE_MISSES_LIMIT=1000
            API_WEB3_JSON_RPC_API_NAMESPACES=debug
            API_WEB3_JSON_RPC_EXTENDED_API_TRACING=true
            API_WEB3_JSON_RPC_WHITELISTED_TOKENS_FOR_AA="0x0000000000000000000000000000000000000001,0x0000000000000000000000000000000000000002"
            API_WEB3_JSON_RPC_ESTIMATE_GAS_SCALE_FACTOR=1.0
            API_WEB3_JSON_RPC_ESTIMATE_GAS_ACCEPTABLE_OVERESTIMATION=1000
            API_WEB3_JSON_RPC_MAX_TX_SIZE=1000000
            API_WEB3_JSON_RPC_VM_CONCURRENCY_LIMIT=512
            API_WEB3_JSON_RPC_FACTORY_DEPS_CACHE_SIZE_MB=128
            API_WEB3_JSON_RPC_INITIAL_WRITES_CACHE_SIZE_MB=32
            API_WEB3_JSON_RPC_LATEST_VALUES_CACHE_SIZE_MB=256
            API_WEB3_JSON_RPC_LATEST_VALUES_MAX_BLOCK_LAG=50
            API_WEB3_JSON_RPC_FEE_HISTORY_LIMIT=100
            API_WEB3_JSON_RPC_MAX_BATCH_REQUEST_SIZE=200
            API_WEB3_JSON_RPC_WEBSOCKET_REQUESTS_PER_MINUTE_LIMIT=10
            API_WEB3_JSON_RPC_MEMPOOL_CACHE_SIZE=10000
            API_WEB3_JSON_RPC_MEMPOOL_CACHE_UPDATE_INTERVAL=50
            API_CONTRACT_VERIFICATION_PORT="3070"
            API_CONTRACT_VERIFICATION_URL="http://127.0.0.1:3070"
            API_WEB3_JSON_RPC_TREE_API_URL="http://tree/"
            API_WEB3_JSON_RPC_MAX_RESPONSE_BODY_SIZE_MB=10
            API_WEB3_JSON_RPC_MAX_RESPONSE_BODY_SIZE_OVERRIDES_MB="eth_call=1, eth_getTransactionReceipt=None, zks_getProof=32"
            API_PROMETHEUS_LISTENER_PORT="3312"
            API_PROMETHEUS_PUSHGATEWAY_URL="http://127.0.0.1:9091"
            API_PROMETHEUS_PUSH_INTERVAL_MS=100
            API_HEALTHCHECK_PORT=8081
            API_HEALTHCHECK_SLOW_TIME_LIMIT_MS=250
            API_HEALTHCHECK_HARD_TIME_LIMIT_MS=2000
            API_MERKLE_TREE_PORT=8082
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("API_");
        let config = test_complete::<ApiConfig>(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          web3_json_rpc:
            http_port: 3050
            http_url: http://127.0.0.1:3050/
            ws_port: 3051
            ws_url: ws://127.0.0.1:3051/
            req_entities_limit: 10000
            filters_limit: 10000
            fee_history_limit: 100
            subscriptions_limit: 10000
            websocket_requests_per_minute_limit: 10
            vm_concurrency_limit: 512
            vm_execution_cache_misses_limit: 1000
            max_response_body_size_mb: 10
            max_response_body_size_overrides_mb:
              eth_call: 1
              eth_getTransactionReceipt: null
              zks_getProof: 32
            max_batch_request_size: 200
            initial_writes_cache_size_mb: 32
            factory_deps_cache_size_mb: 128
            latest_values_cache_size_mb: 256
            latest_values_max_block_lag: 50
            mempool_cache_size: 10000
            mempool_cache_update_interval: 50
            pubsub_polling_interval: 200
            max_nonce_ahead: 5
            gas_price_scale_factor: 1.2
            estimate_gas_scale_factor: 1
            estimate_gas_acceptable_overestimation: 1000
            max_tx_size: 1000000
            filters_disabled: false
            api_namespaces:
            - debug
            whitelisted_tokens_for_aa:
            - "0x0000000000000000000000000000000000000001"
            - "0x0000000000000000000000000000000000000002"
            extended_api_tracing: true
            estimate_gas_optimize_search: true
            tree_api_url: "http://tree/"
          prometheus:
            listener_port: 3312
            pushgateway_url: http://127.0.0.1:9091
            push_interval_ms: 100
          healthcheck:
            port: 8081
            slow_time_limit_ms: 250
            hard_time_limit_ms: 2000
          merkle_tree:
            port: 8082
        "#;

        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config = test_complete::<ApiConfig>(yaml).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_idiomatic_yaml() {
        let yaml = r#"
          web3_json_rpc:
            http_port: 3050
            http_url: http://127.0.0.1:3050/
            ws_port: 3051
            ws_url: ws://127.0.0.1:3051/
            req_entities_limit: 10000
            filters_limit: 10000
            fee_history_limit: 100
            subscriptions_limit: 10000
            websocket_requests_per_minute_limit: 10
            vm_concurrency_limit: 512
            vm_execution_cache_misses_limit: 1000
            max_response_body_size: 10 MB
            max_response_body_size_overrides_mb:
              eth_call: 1
              eth_getTransactionReceipt: null
              zks_getProof: 32
            max_batch_request_size: 200
            initial_writes_cache_size: 32 MB
            factory_deps_cache_size: 128 mb
            latest_values_cache_size: 256mb
            latest_values_max_block_lag: 50
            mempool_cache_size: 10000
            mempool_cache_update_interval: 50
            pubsub_polling_interval: 200ms
            max_nonce_ahead: 5
            gas_price_scale_factor: 1.2
            estimate_gas_scale_factor: 1
            estimate_gas_acceptable_overestimation: 1000
            max_tx_size: 1000000
            filters_disabled: false
            api_namespaces:
            - debug
            whitelisted_tokens_for_aa:
            - "0x0000000000000000000000000000000000000001"
            - "0x0000000000000000000000000000000000000002"
            extended_api_tracing: true
            estimate_gas_optimize_search: true
            tree_api_url: "http://tree/"
          prometheus:
            listener_port: 3312
            pushgateway_url: http://127.0.0.1:9091
            push_interval: 100 ms
          healthcheck:
            port: 8081
            slow_time_limit: 250ms
            hard_time_limit: 2s
          merkle_tree:
            port: 8082
        "#;

        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config = test_complete::<ApiConfig>(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
