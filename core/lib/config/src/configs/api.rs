/// External uses
use serde::Deserialize;
/// Built-in uses
use std::net::SocketAddr;
use std::time::Duration;
// Local uses
use super::envy_load;
pub use crate::configs::PrometheusConfig;
use zksync_basic_types::H256;

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
}

impl ApiConfig {
    pub fn from_env() -> Self {
        Self {
            web3_json_rpc: Web3JsonRpcConfig::from_env(),
            contract_verification: ContractVerificationApiConfig::from_env(),
            prometheus: PrometheusConfig::from_env(),
            healthcheck: HealthCheckConfig::from_env(),
        }
    }
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
    /// Max possible limit of filters to be in the state at once.
    pub filters_limit: Option<u32>,
    /// Max possible limit of subscriptions to be in the state at once.
    pub subscriptions_limit: Option<u32>,
    /// Interval between polling db for pubsub (in ms).
    pub pubsub_polling_interval: Option<u64>,
    /// number of threads per server
    pub threads_per_server: u32,
    /// Tx nonce: how far ahead from the committed nonce can it be.
    pub max_nonce_ahead: u32,
    /// The multiplier to use when suggesting gas price. Should be higher than one,
    /// otherwise if the L1 prices soar, the suggested gas price won't be sufficient to be included in block
    pub gas_price_scale_factor: f64,
    /// Inbound transaction limit used for throttling
    pub transactions_per_sec_limit: Option<u32>,
    /// Timeout for requests (in s)
    pub request_timeout: Option<u64>,
    /// Private keys for accounts managed by node
    pub account_pks: Option<Vec<H256>>,
    /// The factor by which to scale the gasLimit
    pub estimate_gas_scale_factor: f64,
    /// The max possible number of gas that `eth_estimateGas` is allowed to overestimate.
    pub estimate_gas_acceptable_overestimation: u32,
    ///  Max possible size of an ABI encoded tx (in bytes).
    pub max_tx_size: usize,
    /// Max number of cache misses during one VM execution. If the number of cache misses exceeds this value, the api server panics.
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
    /// Override value for the amount of threads used for HTTP RPC server.
    /// If not set, the value from `threads_per_server` is used.
    pub http_threads: Option<u32>,
    /// Override value for the amount of threads used for WebSocket RPC server.
    /// If not set, the value from `threads_per_server` is used.
    pub ws_threads: Option<u32>,
    /// Limit for fee history block range.
    pub fee_history_limit: Option<u64>,
    /// Maximum number of requests in a single batch JSON RPC request. Default is 500.
    pub max_batch_request_size: Option<usize>,
    /// Maximum response body size in MiBs. Default is 10 MiB.
    pub max_response_body_size_mb: Option<usize>,
}

impl Web3JsonRpcConfig {
    pub fn from_env() -> Self {
        envy_load("web3_json_rpc", "API_WEB3_JSON_RPC_")
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

    pub fn http_server_threads(&self) -> usize {
        self.http_threads.unwrap_or(self.threads_per_server) as usize
    }

    pub fn ws_server_threads(&self) -> usize {
        self.ws_threads.unwrap_or(self.threads_per_server) as usize
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
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct HealthCheckConfig {
    /// Port to which the REST server is listening.
    pub port: u16,
}

impl HealthCheckConfig {
    pub fn from_env() -> Self {
        envy_load("healthcheck", "API_HEALTHCHECK_")
    }

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
    /// number of threads per server
    pub threads_per_server: u32,
}

impl ContractVerificationApiConfig {
    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new("0.0.0.0".parse().unwrap(), self.port)
    }

    pub fn from_env() -> Self {
        envy_load("contract_verification", "API_CONTRACT_VERIFICATION_")
    }
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use super::*;
    use crate::configs::test_utils::{hash, EnvMutex};

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> ApiConfig {
        ApiConfig {
            web3_json_rpc: Web3JsonRpcConfig {
                http_port: 3050,
                http_url: "http://127.0.0.1:3050".into(),
                ws_port: 3051,
                ws_url: "ws://127.0.0.1:3051".into(),
                req_entities_limit: Some(10000),
                filters_limit: Some(10000),
                subscriptions_limit: Some(10000),
                pubsub_polling_interval: Some(200),
                threads_per_server: 128,
                max_nonce_ahead: 5,
                transactions_per_sec_limit: Some(1000),
                request_timeout: Some(10),
                account_pks: Some(vec![
                    hash("0x0000000000000000000000000000000000000000000000000000000000000001"),
                    hash("0x0000000000000000000000000000000000000000000000000000000000000002"),
                ]),
                estimate_gas_scale_factor: 1.0f64,
                gas_price_scale_factor: 1.2,
                estimate_gas_acceptable_overestimation: 1000,
                max_tx_size: 1000000,
                vm_execution_cache_misses_limit: None,
                vm_concurrency_limit: Some(512),
                factory_deps_cache_size_mb: Some(128),
                initial_writes_cache_size_mb: Some(32),
                latest_values_cache_size_mb: Some(256),
                http_threads: Some(128),
                ws_threads: Some(256),
                fee_history_limit: Some(100),
                max_batch_request_size: Some(200),
                max_response_body_size_mb: Some(10),
            },
            contract_verification: ContractVerificationApiConfig {
                port: 3070,
                url: "http://127.0.0.1:3070".into(),
                threads_per_server: 128,
            },
            prometheus: PrometheusConfig {
                listener_port: 3312,
                pushgateway_url: "http://127.0.0.1:9091".into(),
                push_interval_ms: Some(100),
            },
            healthcheck: HealthCheckConfig { port: 8081 },
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            API_WEB3_JSON_RPC_HTTP_PORT="3050"
            API_WEB3_JSON_RPC_HTTP_URL="http://127.0.0.1:3050"
            API_WEB3_JSON_RPC_WS_PORT="3051"
            API_WEB3_JSON_RPC_WS_URL="ws://127.0.0.1:3051"
            API_WEB3_JSON_RPC_REQ_ENTITIES_LIMIT=10000
            API_WEB3_JSON_RPC_FILTERS_LIMIT=10000
            API_WEB3_JSON_RPC_SUBSCRIPTIONS_LIMIT=10000
            API_WEB3_JSON_RPC_PUBSUB_POLLING_INTERVAL=200
            API_WEB3_JSON_RPC_THREADS_PER_SERVER=128
            API_WEB3_JSON_RPC_MAX_NONCE_AHEAD=5
            API_WEB3_JSON_RPC_GAS_PRICE_SCALE_FACTOR=1.2
            API_WEB3_JSON_RPC_TRANSACTIONS_PER_SEC_LIMIT=1000
            API_WEB3_JSON_RPC_REQUEST_TIMEOUT=10
            API_WEB3_JSON_RPC_ACCOUNT_PKS=0x0000000000000000000000000000000000000000000000000000000000000001,0x0000000000000000000000000000000000000000000000000000000000000002
            API_WEB3_JSON_RPC_ESTIMATE_GAS_SCALE_FACTOR=1.0
            API_WEB3_JSON_RPC_ESTIMATE_GAS_ACCEPTABLE_OVERESTIMATION=1000
            API_WEB3_JSON_RPC_MAX_TX_SIZE=1000000
            API_WEB3_JSON_RPC_VM_CONCURRENCY_LIMIT=512
            API_WEB3_JSON_RPC_FACTORY_DEPS_CACHE_SIZE_MB=128
            API_WEB3_JSON_RPC_INITIAL_WRITES_CACHE_SIZE_MB=32
            API_WEB3_JSON_RPC_LATEST_VALUES_CACHE_SIZE_MB=256
            API_WEB3_JSON_RPC_HTTP_THREADS=128
            API_WEB3_JSON_RPC_WS_THREADS=256
            API_WEB3_JSON_RPC_FEE_HISTORY_LIMIT=100
            API_WEB3_JSON_RPC_MAX_BATCH_REQUEST_SIZE=200
            API_CONTRACT_VERIFICATION_PORT="3070"
            API_CONTRACT_VERIFICATION_URL="http://127.0.0.1:3070"
            API_CONTRACT_VERIFICATION_THREADS_PER_SERVER=128
            API_WEB3_JSON_RPC_MAX_RESPONSE_BODY_SIZE_MB=10
            API_PROMETHEUS_LISTENER_PORT="3312"
            API_PROMETHEUS_PUSHGATEWAY_URL="http://127.0.0.1:9091"
            API_PROMETHEUS_PUSH_INTERVAL_MS=100
            API_HEALTHCHECK_PORT=8081
        "#;
        lock.set_env(config);

        let actual = ApiConfig::from_env();
        assert_eq!(actual, expected_config());
    }

    /// Checks the correctness of the config helper methods.
    #[test]
    fn methods() {
        let config = expected_config();
        let bind_broadcast_addr: IpAddr = "0.0.0.0".parse().unwrap();

        assert_eq!(
            config.web3_json_rpc.pubsub_interval(),
            Duration::from_millis(200)
        );
        assert_eq!(
            config.contract_verification.bind_addr(),
            SocketAddr::new(bind_broadcast_addr, config.contract_verification.port)
        );
    }
}
