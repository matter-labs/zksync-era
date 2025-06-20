use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    num::{NonZeroU32, NonZeroUsize},
    str::FromStr,
    time::Duration,
};

use anyhow::Context as _;
use serde::{de, ser, Deserialize, Deserializer, Serialize, Serializer};
use smart_config::{
    de::{Delimited, Entries, NamedEntries, OrString, Serde, ToEntries, WellKnown},
    metadata::{SizeUnit, TimeUnit},
    ByteSize, DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::Address;

use crate::utils::Fallback;

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

impl ApiConfig {
    pub fn for_tests() -> Self {
        Self {
            web3_json_rpc: Web3JsonRpcConfig::default(),
            healthcheck: HealthCheckConfig {
                port: 3052.into(),
                slow_time_limit: None,
                hard_time_limit: None,
                expose_config: false,
            },
            merkle_tree: MerkleTreeApiConfig { port: 3053 },
        }
    }
}

/// Port binding specification.
#[derive(Debug, Clone, PartialEq)]
pub enum BindAddress {
    Tcp(SocketAddr),
    #[cfg(unix)]
    Unix(std::path::PathBuf),
}

impl BindAddress {
    #[cfg(unix)]
    const EXPECTING: &'static str = "port number (to bind to 0.0.0.0), socket address or path to the unix socket prefixed by 'unix:'";
    #[cfg(not(unix))]
    const EXPECTING: &'static str = "port number (to bind to 0.0.0.0) or socket address";
}

impl From<u16> for BindAddress {
    fn from(port: u16) -> Self {
        Self::Tcp(SocketAddr::new([0, 0, 0, 0].into(), port))
    }
}

impl From<SocketAddr> for BindAddress {
    fn from(addr: SocketAddr) -> Self {
        Self::Tcp(addr)
    }
}

impl Serialize for BindAddress {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Tcp(addr) => {
                if addr.ip().is_unspecified() {
                    addr.port().serialize(serializer)
                } else {
                    addr.serialize(serializer)
                }
            }
            #[cfg(unix)]
            Self::Unix(path) => {
                let path = path
                    .to_str()
                    .ok_or_else(|| ser::Error::custom("path cannot be encoded to UTF-8"))?;
                format!("unix:{path}").serialize(serializer)
            }
        }
    }
}

impl<'de> Deserialize<'de> for BindAddress {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum SerdePort {
            Just(u16),
            Tcp(SocketAddr),
            String(String),
        }

        Ok(match SerdePort::deserialize(deserializer)? {
            SerdePort::Just(port) => port.into(),
            SerdePort::Tcp(addr) => addr.into(),
            SerdePort::String(s) => {
                #[cfg(unix)]
                if let Some(path) = s.strip_prefix("unix:") {
                    return Ok(Self::Unix(path.into()));
                }

                if let Ok(port) = s.parse::<u16>() {
                    Self::from(port) // Necessary to support parsing from env vars
                } else {
                    return Err(de::Error::invalid_value(
                        de::Unexpected::Str(&s),
                        &Self::EXPECTING,
                    ));
                }
            }
        })
    }
}

impl WellKnown for BindAddress {
    type Deserializer = Serde![int, str];
    const DE: Self::Deserializer = Serde![int, str];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Namespace {
    Eth,
    Net,
    Web3,
    Debug,
    Zks,
    En,
    Pubsub,
    Snapshots,
    Unstable,
}

impl Namespace {
    pub const DEFAULT: [Self; 6] = [
        Self::Eth,
        Self::Net,
        Self::Web3,
        Self::Zks,
        Self::En,
        Self::Pubsub,
    ];
}

impl WellKnown for Namespace {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
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

impl ToEntries<String, Option<NonZeroUsize>> for MaxResponseSizeOverrides {
    fn to_entries(&self) -> impl Iterator<Item = (&String, &Option<NonZeroUsize>)> {
        self.0.iter()
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
    type Deserializer = OrString<NamedEntries<String, Option<NonZeroUsize>>>;
    const DE: Self::Deserializer = OrString(Entries::WELL_KNOWN.named("method", "size_mb"));
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
    /// Interval between polling the node database for subscriptions.
    #[config(default_t = Duration::from_millis(200), with = Fallback(TimeUnit::Millis))]
    pub pubsub_polling_interval: Duration,
    /// Tx nonce: how far ahead from the committed nonce can it be.
    #[config(default_t = 50)]
    pub max_nonce_ahead: u32,
    /// The multiplier to use when suggesting gas price. Should be higher than one,
    /// otherwise if the L1 prices soar, the suggested gas price won't be sufficient to be included in block.
    /// This value is only used when there is no open batch.
    #[config(default_t = 1.5, validate(1.0.., "must be higher than one"))]
    pub gas_price_scale_factor: f64,
    /// The factor by which to scale the gas price when there is an open batch.
    #[config(validate(1.0.., "must be higher than one"))]
    pub gas_price_scale_factor_open_batch: Option<f64>,
    /// The factor by which to scale the gasLimit
    #[config(default_t = 1.3, validate(1.0.., "must be higher than one"))]
    pub estimate_gas_scale_factor: f64,
    /// The max possible number of gas that `eth_estimateGas` is allowed to overestimate.
    #[config(default_t = 1_000)]
    pub estimate_gas_acceptable_overestimation: u32,
    /// Enables optimizations for the binary search of the gas limit in `eth_estimateGas`. These optimizations are currently
    /// considered experimental.
    #[config(default)]
    pub estimate_gas_optimize_search: bool,
    /// Max possible size of an ABI-encoded transaction.
    #[config(default_t = 10 * SizeUnit::MiB, with = Fallback(SizeUnit::Bytes))]
    pub max_tx_size: ByteSize,
    /// Max number of cache misses during one VM execution. If the number of cache misses exceeds this value, the VM execution is stopped.
    pub vm_execution_cache_misses_limit: Option<usize>,
    /// Max number of VM instances to be concurrently spawned by the API server.
    /// This option can be tweaked down if the API server is running out of memory.
    #[config(default_t = 2_048)]
    pub vm_concurrency_limit: usize,
    /// Smart contract cache size.
    #[config(default_t = 128 * SizeUnit::MiB)]
    pub factory_deps_cache_size: ByteSize,
    /// Initial writes cache size.
    #[config(default_t = 32 * SizeUnit::MiB)]
    pub initial_writes_cache_size: ByteSize,
    /// Latest values cache size. If set to 0, the latest values cache will be disabled.
    #[config(default_t = 128 * SizeUnit::MiB)]
    pub latest_values_cache_size: ByteSize,
    /// Maximum lag in the number of blocks for the latest values cache after which the cache is reset. Greater values
    /// lead to increased the cache update latency, i.e., less storage queries being processed by the cache. OTOH, smaller values
    /// can lead to spurious resets when Postgres lags for whatever reason (e.g., when sealing L1 batches).
    #[config(default_t = NonZeroU32::new(20).unwrap())]
    pub latest_values_max_block_lag: NonZeroU32,
    /// Limit for fee history block range.
    #[config(default_t = 1_024)]
    pub fee_history_limit: u64,
    /// Maximum number of requests in a single batch JSON RPC request. Default is 500.
    #[config(default_t = NonZeroUsize::new(500).unwrap())]
    pub max_batch_request_size: NonZeroUsize,
    /// Maximum response body size. Note that there are overrides (`max_response_body_size_overrides_mb`)
    /// taking precedence over this param.
    #[config(default_t = 10 * SizeUnit::MiB)]
    pub max_response_body_size: ByteSize,
    /// Method-specific overrides in MiBs for the maximum response body size.
    #[config(default = MaxResponseSizeOverrides::empty)]
    #[config(alias = "max_response_body_size_overrides_mb")]
    pub max_response_body_size_overrides: MaxResponseSizeOverrides,
    /// Maximum number of requests per minute for the WebSocket server.
    /// The value is per active connection.
    /// Not used for the HTTP server; for it, rate limiting is expected to be configured on the infra level.
    #[config(default_t = NonZeroU32::new(6_000).unwrap())]
    pub websocket_requests_per_minute_limit: NonZeroU32,
    /// Server-side request timeout. A request will be dropped with a 503 error code if its execution exceeds this limit.
    /// If not specified, no server-side request timeout is enforced.
    pub request_timeout: Option<Duration>,
    /// Tree API URL used to proxy `getProof` calls to the tree. For external nodes, it's not necessary to specify
    /// since the server can communicate with the tree in-process.
    #[config(alias = "tree_api_remote_url")]
    pub tree_api_url: Option<String>,
    /// Polling period for mempool cache update - how often the mempool cache is updated from the database.
    #[config(default_t = Duration::from_millis(50), with = Fallback(TimeUnit::Millis))]
    pub mempool_cache_update_interval: Duration,
    /// Maximum number of transactions to be stored in the mempool cache.
    #[config(default_t = 10_000)]
    pub mempool_cache_size: usize,
    /// List of L2 token addresses that are white-listed to use by paymasters
    /// (additionally to natively bridged tokens).
    #[config(default, with = Delimited(","))]
    pub whitelisted_tokens_for_aa: Vec<Address>,
    /// Enabled JSON RPC API namespaces.
    #[config(with = Delimited(","), default_t = Namespace::DEFAULT.into())]
    pub api_namespaces: HashSet<Namespace>,
    /// Enables extended tracing of RPC calls. This is useful for debugging, but may negatively impact performance for nodes under high load
    /// (hundreds or thousands RPS).
    #[config(default, alias = "extended_rpc_tracing")]
    pub extended_api_tracing: bool,
}

impl Web3JsonRpcConfig {
    /// Creates a mock instance of `Web3JsonRpcConfig` to be used in tests.
    /// Ports and some fields that may affect execution are set to the same values used by default in
    /// the localhost environment. Other fields are set to default values.
    pub fn for_tests() -> Self {
        Self {
            gas_price_scale_factor: 1.2,
            gas_price_scale_factor_open_batch: Some(1.2),
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
            global: self.max_response_body_size.0 as usize,
            overrides: self.max_response_body_size_overrides.scale(scale),
        }
    }
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct HealthCheckConfig {
    /// Port to which the healthcheck server is listening.
    pub port: BindAddress,
    /// Time limit in milliseconds to mark a health check as slow and log the corresponding warning.
    /// If not specified, the default value in the health check crate will be used.
    pub slow_time_limit: Option<Duration>,
    /// Time limit in milliseconds to abort a health check and return "not ready" status for the corresponding component.
    /// If not specified, the default value in the health check crate will be used.
    pub hard_time_limit: Option<Duration>,
    /// Expose config parameters as the `config` component. Mostly useful for debugging purposes, automations or end-to-end testing.
    #[config(default)]
    pub expose_config: bool,
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
    use smart_config::{
        testing::{test, test_complete},
        Environment, Yaml,
    };

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
                max_tx_size: ByteSize(1000000),
                vm_execution_cache_misses_limit: Some(1000),
                vm_concurrency_limit: 512,
                factory_deps_cache_size: ByteSize::new(128, SizeUnit::MiB),
                initial_writes_cache_size: ByteSize::new(32, SizeUnit::MiB),
                latest_values_cache_size: ByteSize::new(256, SizeUnit::MiB),
                latest_values_max_block_lag: NonZeroU32::new(50).unwrap(),
                fee_history_limit: 100,
                max_batch_request_size: NonZeroUsize::new(200).unwrap(),
                max_response_body_size: ByteSize::new(15, SizeUnit::MiB),
                max_response_body_size_overrides: [
                    ("eth_call", NonZeroUsize::new(1)),
                    ("eth_getTransactionReceipt", None),
                    ("zks_getProof", NonZeroUsize::new(32)),
                ]
                .into_iter()
                .collect(),
                websocket_requests_per_minute_limit: NonZeroU32::new(10).unwrap(),
                request_timeout: Some(Duration::from_secs(20)),
                tree_api_url: Some("http://tree/".into()),
                mempool_cache_update_interval: Duration::from_millis(50),
                mempool_cache_size: 10000,
                whitelisted_tokens_for_aa: vec![
                    Address::from_low_u64_be(1),
                    Address::from_low_u64_be(2),
                ],
                api_namespaces: HashSet::from([Namespace::Debug]),
                extended_api_tracing: true,
                gas_price_scale_factor_open_batch: Some(1.3),
            },
            healthcheck: HealthCheckConfig {
                port: 8081.into(),
                slow_time_limit: Some(Duration::from_millis(250)),
                hard_time_limit: Some(Duration::from_millis(2_000)),
                expose_config: true,
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
            API_WEB3_JSON_RPC_GAS_PRICE_SCALE_FACTOR_OPEN_BATCH=1.3
            API_WEB3_JSON_RPC_ESTIMATE_GAS_OPTIMIZE_SEARCH=true
            API_WEB3_JSON_RPC_VM_EXECUTION_CACHE_MISSES_LIMIT=1000
            API_WEB3_JSON_RPC_API_NAMESPACES=debug
            API_WEB3_JSON_RPC_EXTENDED_API_TRACING=true
            API_WEB3_JSON_RPC_WHITELISTED_TOKENS_FOR_AA="0x0000000000000000000000000000000000000001,0x0000000000000000000000000000000000000002"
            API_WEB3_JSON_RPC_ESTIMATE_GAS_SCALE_FACTOR=1.0
            API_WEB3_JSON_RPC_ESTIMATE_GAS_ACCEPTABLE_OVERESTIMATION=1000
            API_WEB3_JSON_RPC_MAX_TX_SIZE=1000000
            API_WEB3_JSON_RPC_VM_CONCURRENCY_LIMIT=512
            API_WEB3_JSON_RPC_REQUEST_TIMEOUT="20 sec"
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
            API_WEB3_JSON_RPC_MAX_RESPONSE_BODY_SIZE_MB=15
            API_WEB3_JSON_RPC_MAX_RESPONSE_BODY_SIZE_OVERRIDES_MB="eth_call=1, eth_getTransactionReceipt=None, zks_getProof=32"
            API_PROMETHEUS_LISTENER_PORT="3312"
            API_PROMETHEUS_PUSHGATEWAY_URL="http://127.0.0.1:9091"
            API_PROMETHEUS_PUSH_INTERVAL_MS=100
            API_HEALTHCHECK_PORT=8081
            API_HEALTHCHECK_SLOW_TIME_LIMIT_MS=250
            API_HEALTHCHECK_HARD_TIME_LIMIT_MS=2000
            API_HEALTHCHECK_EXPOSE_CONFIG=true
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
            max_response_body_size_mb: 15
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
            gas_price_scale_factor_open_batch: 1.3
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
            request_timeout_sec: 20
            tree_api_url: "http://tree/"
          prometheus:
            listener_port: 3312
            pushgateway_url: http://127.0.0.1:9091
            push_interval_ms: 100
          healthcheck:
            port: 8081
            slow_time_limit_ms: 250
            hard_time_limit_ms: 2000
            expose_config: true
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
            max_response_body_size: 15 MB
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
            gas_price_scale_factor_open_batch: 1.3
            estimate_gas_scale_factor: 1
            estimate_gas_acceptable_overestimation: 1000
            max_tx_size: 1000000 B
            filters_disabled: false
            api_namespaces:
            - debug
            whitelisted_tokens_for_aa:
            - "0x0000000000000000000000000000000000000001"
            - "0x0000000000000000000000000000000000000002"
            extended_api_tracing: true
            estimate_gas_optimize_search: true
            request_timeout: 20s
            tree_api_url: "http://tree/"
          prometheus:
            listener_port: 3312
            pushgateway_url: http://127.0.0.1:9091
            push_interval: 100 ms
          healthcheck:
            port: 8081
            slow_time_limit: 250ms
            hard_time_limit: 2s
            expose_config: true
          merkle_tree:
            port: 8082
        "#;

        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config = test_complete::<ApiConfig>(yaml).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_null_time_limits() {
        let yaml = r#"
          port: 3071
          slow_time_limit_ms: null
          hard_time_limit_ms: null
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config = test::<HealthCheckConfig>(yaml).unwrap();
        assert_eq!(config.slow_time_limit, None);
        assert_eq!(config.hard_time_limit, None);
    }

    #[test]
    fn parsing_full_address_binding() {
        let yaml = r#"
          port: 127.0.0.1:3050
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config = test::<HealthCheckConfig>(yaml).unwrap();
        assert_eq!(config.port, BindAddress::Tcp(([127, 0, 0, 1], 3050).into()));
    }

    #[cfg(unix)]
    #[test]
    fn parsing_unix_domain_socket_binding() {
        let yaml = r#"
          port: unix:/var/era/health.sock
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config = test::<HealthCheckConfig>(yaml).unwrap();
        assert_eq!(
            config.port,
            BindAddress::Unix("/var/era/health.sock".into())
        );
    }

    #[test]
    fn port_roundtrip() {
        let port = BindAddress::from(3050);
        let json = serde_json::to_value(port.clone()).unwrap();
        assert_eq!(json, serde_json::json!(3050));
        assert_eq!(serde_json::from_value::<BindAddress>(json).unwrap(), port);

        let port = BindAddress::Tcp(([10, 10, 0, 1], 3050).into());
        let json = serde_json::to_value(port.clone()).unwrap();
        assert_eq!(json, serde_json::json!("10.10.0.1:3050"));
        assert_eq!(serde_json::from_value::<BindAddress>(json).unwrap(), port);
    }

    #[cfg(unix)]
    #[test]
    fn unix_port_roundtrip() {
        let port = BindAddress::Unix("/var/node.sock".into());
        let json = serde_json::to_value(port.clone()).unwrap();
        assert_eq!(json, serde_json::json!("unix:/var/node.sock"));
        assert_eq!(serde_json::from_value::<BindAddress>(json).unwrap(), port);
    }

    #[test]
    fn parsing_max_response_overrides() {
        let yaml = r#"
          max_response_body_size_overrides:
           - method: eth_getTransactionReceipt
           - method: zks_getProof
             size_mb: 64
          max_response_body_size_mb: 100
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config = test::<Web3JsonRpcConfig>(yaml).unwrap();
        assert_eq!(
            config.max_response_body_size_overrides,
            MaxResponseSizeOverrides::from_iter([
                ("eth_getTransactionReceipt", None),
                ("zks_getProof", NonZeroUsize::new(64)),
            ])
        );
    }
}
