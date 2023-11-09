pub use crate::configs::PrometheusConfig;
use anyhow::Context as _;
use serde::Deserialize;
use std::{
    convert::{TryFrom as _, TryInto as _},
    net::SocketAddr,
    time::Duration,
};
use zksync_basic_types::H256;
use zksync_protobuf::{read_required, required, ProtoFmt};

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

impl ProtoFmt for ApiConfig {
    type Proto = super::proto::Api;
    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        Ok(Self {
            web3_json_rpc: read_required(&r.web3_json_rpc).context("web3_json_rpc")?,
            contract_verification: read_required(&r.contract_verification)
                .context("contract_verification")?,
            prometheus: read_required(&r.prometheus).context("prometheus")?,
            healthcheck: read_required(&r.healthcheck).context("healthcheck")?,
            merkle_tree: read_required(&r.merkle_tree).context("merkle_tree")?,
        })
    }

    fn build(&self) -> Self::Proto {
        Self::Proto {
            web3_json_rpc: Some(self.web3_json_rpc.build()),
            contract_verification: Some(self.contract_verification.build()),
            prometheus: Some(self.prometheus.build()),
            healthcheck: Some(self.healthcheck.build()),
            merkle_tree: Some(self.merkle_tree.build()),
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
    /// Maximum number of requests per minute for the WebSocket server.
    /// The value is per active connection.
    /// Note: For HTTP, rate limiting is expected to be configured on the infra level.
    pub websocket_requests_per_minute_limit: Option<u32>,
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
            filters_limit: Some(10000),
            subscriptions_limit: Some(10000),
            pubsub_polling_interval: Some(200),
            threads_per_server: 1,
            max_nonce_ahead: 50,
            gas_price_scale_factor: 1.2,
            transactions_per_sec_limit: Default::default(),
            request_timeout: Default::default(),
            account_pks: Default::default(),
            estimate_gas_scale_factor: 1.2,
            estimate_gas_acceptable_overestimation: 1000,
            max_tx_size: 1000000,
            vm_execution_cache_misses_limit: Default::default(),
            vm_concurrency_limit: Default::default(),
            factory_deps_cache_size_mb: Default::default(),
            initial_writes_cache_size_mb: Default::default(),
            latest_values_cache_size_mb: Default::default(),
            http_threads: Default::default(),
            ws_threads: Default::default(),
            fee_history_limit: Default::default(),
            max_batch_request_size: Default::default(),
            max_response_body_size_mb: Default::default(),
            websocket_requests_per_minute_limit: Default::default(),
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

    pub fn websocket_requests_per_minute_limit(&self) -> u32 {
        // The default limit is chosen to be reasonably permissive.
        self.websocket_requests_per_minute_limit.unwrap_or(6000)
    }
}

impl ProtoFmt for Web3JsonRpcConfig {
    type Proto = super::proto::Web3JsonRpc;
    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        Ok(Self {
            http_port: required(&r.http_port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("http_port")?,
            http_url: required(&r.http_url).context("http_url")?.clone(),
            ws_port: required(&r.ws_port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("ws_port")?,
            ws_url: required(&r.ws_url).context("ws_url")?.clone(),
            req_entities_limit: r.req_entities_limit,
            filters_limit: r.filters_limit,
            subscriptions_limit: r.subscriptions_limit,
            pubsub_polling_interval: r.pubsub_polling_interval,
            threads_per_server: *required(&r.threads_per_server).context("threads_per_server")?,
            max_nonce_ahead: *required(&r.max_nonce_ahead).context("max_nonce_ahead")?,
            gas_price_scale_factor: *required(&r.gas_price_scale_factor)
                .context("gas_price_scale_factor")?,
            transactions_per_sec_limit: r.transactions_per_sec_limit,
            request_timeout: r.request_timeout,
            account_pks: match &r.account_pks {
                None => None,
                Some(r) => {
                    let mut keys = vec![];
                    for (i, k) in r.keys.iter().enumerate() {
                        keys.push(
                            <[u8; 32]>::try_from(&k[..])
                                .with_context(|| format!("keys[{i}]"))?
                                .into(),
                        );
                    }
                    Some(keys)
                }
            },
            estimate_gas_scale_factor: *required(&r.estimate_gas_scale_factor)
                .context("estimate_gas_scale_factor")?,
            estimate_gas_acceptable_overestimation: *required(
                &r.estimate_gas_acceptable_overestimation,
            )
            .context("acceptable_overestimation")?,
            max_tx_size: required(&r.max_tx_size)
                .and_then(|x| Ok((*x).try_into()?))
                .context("max_tx_size")?,
            vm_execution_cache_misses_limit: r
                .vm_execution_cache_misses_limit
                .map(|x| x.try_into())
                .transpose()
                .context("vm_execution_cache_misses_limit")?,
            vm_concurrency_limit: r
                .vm_concurrency_limit
                .map(|x| x.try_into())
                .transpose()
                .context("vm_concurrency_limit")?,
            factory_deps_cache_size_mb: r
                .factory_deps_cache_size_mb
                .map(|x| x.try_into())
                .transpose()
                .context("factory_deps_cache_size_mb")?,
            initial_writes_cache_size_mb: r
                .initial_writes_cache_size_mb
                .map(|x| x.try_into())
                .transpose()
                .context("initial_writes_cache_size_mb")?,
            latest_values_cache_size_mb: r
                .latest_values_cache_size_mb
                .map(|x| x.try_into())
                .transpose()
                .context("latests_values_cache_size_mb")?,
            http_threads: r.http_threads,
            ws_threads: r.ws_threads,
            fee_history_limit: r.fee_history_limit,
            max_batch_request_size: r
                .max_batch_request_size
                .map(|x| x.try_into())
                .transpose()
                .context("max_batch_requres_size")?,
            max_response_body_size_mb: r
                .max_response_body_size_mb
                .map(|x| x.try_into())
                .transpose()
                .context("max_response_body_size_mb")?,
            websocket_requests_per_minute_limit: r.websocket_requests_per_minute_limit,
        })
    }
    fn build(&self) -> Self::Proto {
        Self::Proto {
            http_port: Some(self.http_port.into()),
            http_url: Some(self.http_url.clone()),
            ws_port: Some(self.ws_port.into()),
            ws_url: Some(self.ws_url.clone()),
            req_entities_limit: self.req_entities_limit,
            filters_limit: self.filters_limit,
            subscriptions_limit: self.subscriptions_limit,
            pubsub_polling_interval: self.pubsub_polling_interval,
            threads_per_server: Some(self.threads_per_server),
            max_nonce_ahead: Some(self.max_nonce_ahead),
            gas_price_scale_factor: Some(self.gas_price_scale_factor),
            transactions_per_sec_limit: self.transactions_per_sec_limit,
            request_timeout: self.request_timeout,
            account_pks: self
                .account_pks
                .as_ref()
                .map(|keys| super::proto::PrivateKeys {
                    keys: keys.iter().map(|k| k.as_bytes().into()).collect(),
                }),
            estimate_gas_scale_factor: Some(self.estimate_gas_scale_factor),
            estimate_gas_acceptable_overestimation: Some(
                self.estimate_gas_acceptable_overestimation,
            ),
            max_tx_size: Some(self.max_tx_size.try_into().unwrap()),
            vm_execution_cache_misses_limit: self
                .vm_execution_cache_misses_limit
                .map(|x| x.try_into().unwrap()),
            vm_concurrency_limit: self.vm_concurrency_limit.map(|x| x.try_into().unwrap()),
            factory_deps_cache_size_mb: self
                .factory_deps_cache_size_mb
                .map(|x| x.try_into().unwrap()),
            initial_writes_cache_size_mb: self
                .initial_writes_cache_size_mb
                .map(|x| x.try_into().unwrap()),
            latest_values_cache_size_mb: self
                .latest_values_cache_size_mb
                .map(|x| x.try_into().unwrap()),
            http_threads: self.http_threads,
            ws_threads: self.ws_threads,
            fee_history_limit: self.fee_history_limit,
            max_batch_request_size: self.max_batch_request_size.map(|x| x.try_into().unwrap()),
            max_response_body_size_mb: self
                .max_response_body_size_mb
                .map(|x| x.try_into().unwrap()),
            websocket_requests_per_minute_limit: self.websocket_requests_per_minute_limit,
        }
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct HealthCheckConfig {
    /// Port to which the REST server is listening.
    pub port: u16,
}

impl HealthCheckConfig {
    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new("0.0.0.0".parse().unwrap(), self.port)
    }
}

impl ProtoFmt for HealthCheckConfig {
    type Proto = super::proto::HealthCheck;
    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        Ok(Self {
            port: required(&r.port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("port")?,
        })
    }
    fn build(&self) -> Self::Proto {
        Self::Proto {
            port: Some(self.port.into()),
        }
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
}

impl ProtoFmt for ContractVerificationApiConfig {
    type Proto = super::proto::ContractVerificationApi;
    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        Ok(Self {
            port: required(&r.port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("port")?,
            url: required(&r.url).context("url")?.clone(),
            threads_per_server: *required(&r.threads_per_server).context("threads_per_server")?,
        })
    }
    fn build(&self) -> Self::Proto {
        Self::Proto {
            port: Some(self.port.into()),
            url: Some(self.url.clone()),
            threads_per_server: Some(self.threads_per_server),
        }
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

impl ProtoFmt for MerkleTreeApiConfig {
    type Proto = super::proto::MerkleTreeApi;
    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        Ok(Self {
            port: required(&r.port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("port")?,
        })
    }
    fn build(&self) -> Self::Proto {
        Self::Proto {
            port: Some(self.port.into()),
        }
    }
}
