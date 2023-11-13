use anyhow::Context;
use serde::Deserialize;
use std::{env, time::Duration};
use url::Url;

use zksync_basic_types::{Address, L1ChainId, L2ChainId, MiniblockNumber};
use zksync_core::api_server::{
    tx_sender::TxSenderConfig, web3::state::InternalApiConfig, web3::Namespace,
};
use zksync_types::api::BridgeAddresses;

use zksync_web3_decl::{
    jsonrpsee::http_client::{HttpClient, HttpClientBuilder},
    namespaces::{EnNamespaceClient, EthNamespaceClient, ZksNamespaceClient},
};

#[cfg(test)]
mod tests;

const BYTES_IN_MEGABYTE: usize = 1_024 * 1_024;

/// This part of the external node config is fetched directly from the main node.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct RemoteENConfig {
    pub diamond_proxy_addr: Address,
    pub l1_erc20_bridge_proxy_addr: Address,
    pub l2_erc20_bridge_addr: Address,
    pub l1_weth_bridge_proxy_addr: Option<Address>,
    pub l2_weth_bridge_addr: Option<Address>,
    pub l2_testnet_paymaster_addr: Option<Address>,
    pub l2_chain_id: L2ChainId,
    pub l1_chain_id: L1ChainId,

    pub fair_l2_gas_price: u64,
}

impl RemoteENConfig {
    pub async fn fetch(client: &HttpClient) -> anyhow::Result<Self> {
        let bridges = client
            .get_bridge_contracts()
            .await
            .context("Failed to fetch bridge contracts")?;
        let l2_testnet_paymaster_addr = client
            .get_testnet_paymaster()
            .await
            .context("Failed to fetch paymaster")?;
        let diamond_proxy_addr = client
            .get_main_contract()
            .await
            .context("Failed to fetch L1 contract address")?;
        let l2_chain_id = L2ChainId::try_from(
            client
                .chain_id()
                .await
                .context("Failed to fetch L2 chain ID")?
                .as_u64(),
        )
        .unwrap();
        let l1_chain_id = L1ChainId(
            client
                .l1_chain_id()
                .await
                .context("Failed to fetch L1 chain ID")?
                .as_u64(),
        );
        let current_miniblock = client
            .get_block_number()
            .await
            .context("Failed to fetch block number")?;
        let block_header = client
            .sync_l2_block(MiniblockNumber(current_miniblock.as_u32()), false)
            .await
            .context("Failed to fetch last miniblock header")?
            .expect("Block is known to exist");

        Ok(Self {
            diamond_proxy_addr,
            l2_testnet_paymaster_addr,
            l1_erc20_bridge_proxy_addr: bridges.l1_erc20_default_bridge,
            l2_erc20_bridge_addr: bridges.l2_erc20_default_bridge,
            l1_weth_bridge_proxy_addr: bridges.l1_weth_bridge,
            l2_weth_bridge_addr: bridges.l2_weth_bridge,
            l2_chain_id,
            l1_chain_id,
            fair_l2_gas_price: block_header.l2_fair_gas_price,
        })
    }
}

/// This part of the external node config is completely optional to provide.
/// It can tweak limits of the API, delay intervals of certain components, etc.
/// If any of the fields are not provided, the default values will be used.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct OptionalENConfig {
    // User-facing API limits
    /// Max possible limit of filters to be in the API state at once.
    #[serde(default = "OptionalENConfig::default_filters_limit")]
    pub filters_limit: usize,
    /// Max possible limit of subscriptions to be in the API state at once.
    #[serde(default = "OptionalENConfig::default_subscriptions_limit")]
    pub subscriptions_limit: usize,
    /// Max possible limit of entities to be requested via API at once.
    #[serde(default = "OptionalENConfig::default_req_entities_limit")]
    pub req_entities_limit: usize,
    /// Max possible size of an ABI encoded tx (in bytes).
    #[serde(default = "OptionalENConfig::default_max_tx_size")]
    pub max_tx_size: usize,
    /// Max number of cache misses during one VM execution. If the number of cache misses exceeds this value, the api server panics.
    /// This is a temporary solution to mitigate API request resulting in thousands of DB queries.
    pub vm_execution_cache_misses_limit: Option<usize>,
    /// Inbound transaction limit used for throttling.
    pub transactions_per_sec_limit: Option<u32>,
    /// Limit for fee history block range.
    #[serde(default = "OptionalENConfig::default_fee_history_limit")]
    pub fee_history_limit: u64,
    /// Maximum number of requests in a single batch JSON RPC request. Default is 500.
    #[serde(default = "OptionalENConfig::default_max_batch_request_size")]
    pub max_batch_request_size: usize,
    /// Maximum response body size in MiBs. Default is 10 MiB.
    #[serde(default = "OptionalENConfig::default_max_response_body_size_mb")]
    pub max_response_body_size_mb: usize,

    // Other API config settings
    /// Interval between polling DB for pubsub (in ms).
    #[serde(
        rename = "pubsub_polling_interval",
        default = "OptionalENConfig::default_polling_interval"
    )]
    polling_interval: u64,
    /// Tx nonce: how far ahead from the committed nonce can it be.
    #[serde(default = "OptionalENConfig::default_max_nonce_ahead")]
    pub max_nonce_ahead: u32,
    /// Max number of VM instances to be concurrently spawned by the API server.
    /// This option can be tweaked down if the API server is running out of memory.
    #[serde(default = "OptionalENConfig::default_vm_concurrency_limit")]
    pub vm_concurrency_limit: usize,
    /// Smart contract bytecode cache size for the API server. Default value is 128 MiB.
    #[serde(default = "OptionalENConfig::default_factory_deps_cache_size_mb")]
    factory_deps_cache_size_mb: usize,
    /// Initial writes cache size for the API server. Default value is 32 MiB.
    #[serde(default = "OptionalENConfig::default_initial_writes_cache_size_mb")]
    initial_writes_cache_size_mb: usize,
    /// Latest values cache size in MiBs. The default value is 128 MiB. If set to 0, the latest
    /// values cache will be disabled.
    #[serde(default = "OptionalENConfig::default_latest_values_cache_size_mb")]
    latest_values_cache_size_mb: usize,
    /// Enabled JSON RPC API namespaces.
    api_namespaces: Option<Vec<Namespace>>,

    // Gas estimation config
    /// The factor by which to scale the gasLimit
    #[serde(default = "OptionalENConfig::default_estimate_gas_scale_factor")]
    pub estimate_gas_scale_factor: f64,
    /// The max possible number of gas that `eth_estimateGas` is allowed to overestimate.
    #[serde(default = "OptionalENConfig::default_estimate_gas_acceptable_overestimation")]
    pub estimate_gas_acceptable_overestimation: u32,
    /// The multiplier to use when suggesting gas price. Should be higher than one,
    /// otherwise if the L1 prices soar, the suggested gas price won't be sufficient to be included in block
    #[serde(default = "OptionalENConfig::default_gas_price_scale_factor")]
    pub gas_price_scale_factor: f64,

    // Merkle tree config
    #[serde(default = "OptionalENConfig::default_metadata_calculator_delay")]
    metadata_calculator_delay: u64,
    /// Maximum number of L1 batches to be processed by the Merkle tree at a time.
    #[serde(
        alias = "max_blocks_per_tree_batch",
        default = "OptionalENConfig::default_max_l1_batches_per_tree_iter"
    )]
    pub max_l1_batches_per_tree_iter: usize,
    /// Chunk size for multi-get operations. Can speed up loading data for the Merkle tree on some environments,
    /// but the effects vary wildly depending on the setup (e.g., the filesystem used).
    #[serde(default = "OptionalENConfig::default_merkle_tree_multi_get_chunk_size")]
    pub merkle_tree_multi_get_chunk_size: usize,
    /// Capacity of the block cache for the Merkle tree RocksDB. Reasonable values range from ~100 MiB to several GiB.
    /// The default value is 128 MiB.
    #[serde(default = "OptionalENConfig::default_merkle_tree_block_cache_size_mb")]
    merkle_tree_block_cache_size_mb: usize,
    /// Byte capacity of memtables (recent, non-persisted changes to RocksDB). Setting this to a reasonably
    /// large value (order of 512 MiB) is helpful for large DBs that experience write stalls.
    #[serde(default = "OptionalENConfig::default_merkle_tree_memtable_capacity_mb")]
    merkle_tree_memtable_capacity_mb: usize,
    /// Timeout to wait for the Merkle tree database to run compaction on stalled writes.
    #[serde(default = "OptionalENConfig::default_merkle_tree_stalled_writes_timeout_sec")]
    merkle_tree_stalled_writes_timeout_sec: u64,

    // Other config settings
    /// Port on which the Prometheus exporter server is listening.
    pub prometheus_port: Option<u16>,
    /// Number of keys that is processed by enum_index migration in State Keeper each L1 batch.
    #[serde(default = "OptionalENConfig::default_enum_index_migration_chunk_size")]
    pub enum_index_migration_chunk_size: usize,
}

impl OptionalENConfig {
    const fn default_filters_limit() -> usize {
        10_000
    }

    const fn default_subscriptions_limit() -> usize {
        10_000
    }

    const fn default_req_entities_limit() -> usize {
        1_024
    }

    const fn default_max_tx_size() -> usize {
        1_000_000
    }

    const fn default_polling_interval() -> u64 {
        200
    }

    const fn default_estimate_gas_scale_factor() -> f64 {
        1.2
    }

    const fn default_estimate_gas_acceptable_overestimation() -> u32 {
        1_000
    }

    const fn default_gas_price_scale_factor() -> f64 {
        1.2
    }

    const fn default_max_nonce_ahead() -> u32 {
        50
    }

    const fn default_metadata_calculator_delay() -> u64 {
        100
    }

    const fn default_max_l1_batches_per_tree_iter() -> usize {
        20
    }

    const fn default_vm_concurrency_limit() -> usize {
        // The default limit is large so that it does not create a bottleneck on its own.
        // VM execution can still be limited by Tokio runtime parallelism and/or the number
        // of DB connections in a pool.
        2_048
    }

    const fn default_factory_deps_cache_size_mb() -> usize {
        128
    }

    const fn default_initial_writes_cache_size_mb() -> usize {
        32
    }

    const fn default_latest_values_cache_size_mb() -> usize {
        128
    }

    const fn default_merkle_tree_multi_get_chunk_size() -> usize {
        500
    }

    const fn default_merkle_tree_block_cache_size_mb() -> usize {
        128
    }

    const fn default_merkle_tree_memtable_capacity_mb() -> usize {
        256
    }

    const fn default_merkle_tree_stalled_writes_timeout_sec() -> u64 {
        30
    }

    const fn default_fee_history_limit() -> u64 {
        1_024
    }

    const fn default_max_batch_request_size() -> usize {
        500 // The default limit is chosen to be reasonably permissive.
    }

    const fn default_max_response_body_size_mb() -> usize {
        10
    }

    const fn default_enum_index_migration_chunk_size() -> usize {
        5000
    }

    pub fn polling_interval(&self) -> Duration {
        Duration::from_millis(self.polling_interval)
    }

    pub fn metadata_calculator_delay(&self) -> Duration {
        Duration::from_millis(self.metadata_calculator_delay)
    }

    /// Returns the size of factory dependencies cache in bytes.
    pub fn factory_deps_cache_size(&self) -> usize {
        self.factory_deps_cache_size_mb * BYTES_IN_MEGABYTE
    }

    /// Returns the size of initial writes cache in bytes.
    pub fn initial_writes_cache_size(&self) -> usize {
        self.initial_writes_cache_size_mb * BYTES_IN_MEGABYTE
    }

    /// Returns the size of latest values cache in bytes.
    pub fn latest_values_cache_size(&self) -> usize {
        self.latest_values_cache_size_mb * BYTES_IN_MEGABYTE
    }

    /// Returns the size of block cache for Merkle tree in bytes.
    pub fn merkle_tree_block_cache_size(&self) -> usize {
        self.merkle_tree_block_cache_size_mb * BYTES_IN_MEGABYTE
    }

    /// Returns the memtable capacity for Merkle tree in bytes.
    pub fn merkle_tree_memtable_capacity(&self) -> usize {
        self.merkle_tree_memtable_capacity_mb * BYTES_IN_MEGABYTE
    }

    /// Returns the timeout to wait for the Merkle tree database to run compaction on stalled writes.
    pub fn merkle_tree_stalled_writes_timeout(&self) -> Duration {
        Duration::from_secs(self.merkle_tree_stalled_writes_timeout_sec)
    }

    pub fn api_namespaces(&self) -> Vec<Namespace> {
        self.api_namespaces
            .clone()
            .unwrap_or_else(|| Namespace::NON_DEBUG.to_vec())
    }

    pub fn max_response_body_size(&self) -> usize {
        self.max_response_body_size_mb * BYTES_IN_MEGABYTE
    }
}

/// This part of the external node config is required for its operation.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct RequiredENConfig {
    /// Port on which the HTTP RPC server is listening.
    pub http_port: u16,
    /// Port on which the WebSocket RPC server is listening.
    pub ws_port: u16,
    /// Port on which the healthcheck REST server is listening.
    pub healthcheck_port: u16,
    /// Number of threads per API server
    pub threads_per_server: usize,
    /// Address of the Ethereum node API.
    /// Intentionally private: use getter method as it manages the missing port.
    eth_client_url: String,
    /// Main node URL - used by external node to proxy transactions to, query state from, etc.
    /// Intentionally private: use getter method as it manages the missing port.
    main_node_url: String,
    /// Path to the database data directory that serves state cache.
    pub state_cache_path: String,
    /// Fast SSD path. Used as a RocksDB dir for the Merkle tree (*new* implementation).
    pub merkle_tree_path: String,
}

impl RequiredENConfig {
    pub fn main_node_url(&self) -> anyhow::Result<String> {
        Self::get_url(&self.main_node_url).context("Could not parse main node URL")
    }

    pub fn eth_client_url(&self) -> anyhow::Result<String> {
        Self::get_url(&self.eth_client_url).context("Could not parse L1 client URL")
    }

    fn get_url(url_str: &str) -> anyhow::Result<String> {
        let url = Url::parse(url_str).context("URL can not be parsed")?;
        format_url_with_port(&url)
    }
}

/// Configuration for Postgres database.
/// While also mandatory, it historically used different naming scheme for corresponding
/// environment variables.
/// Thus it is kept separately for backward compatibility and ease of deserialization.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PostgresConfig {
    pub database_url: String,
    pub max_connections: u32,
}

impl PostgresConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: env::var("DATABASE_URL")
                .context("DATABASE_URL env variable is not set")?,
            max_connections: env::var("DATABASE_POOL_SIZE")
                .context("DATABASE_POOL_SIZE env variable is not set")?
                .parse()
                .context("Unable to parse DATABASE_POOL_SIZE env variable")?,
        })
    }
}

/// External Node Config contains all the configuration required for the EN operation.
/// It is split into three parts: required, optional and remote for easier navigation.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ExternalNodeConfig {
    pub required: RequiredENConfig,
    pub postgres: PostgresConfig,
    pub optional: OptionalENConfig,
    pub remote: RemoteENConfig,
}

impl ExternalNodeConfig {
    /// Loads config from the environment variables and
    /// fetches contracts addresses from the main node.
    pub async fn collect() -> anyhow::Result<Self> {
        let required = envy::prefixed("EN_")
            .from_env::<RequiredENConfig>()
            .context("could not load external node config")?;

        let optional = envy::prefixed("EN_")
            .from_env::<OptionalENConfig>()
            .context("could not load external node config")?;

        let client = HttpClientBuilder::default()
            .build(required.main_node_url()?)
            .expect("Unable to build HTTP client for main node");
        let remote = RemoteENConfig::fetch(&client)
            .await
            .context("Unable to fetch required config values from the main node")?;

        // We can query them from main node, but it's better to set them explicitly
        // as well to avoid connecting to wrong envs unintentionally.
        let eth_chain_id = HttpClientBuilder::default()
            .build(required.eth_client_url()?)
            .expect("Unable to build HTTP client for L1 client")
            .chain_id()
            .await
            .context("Unable to check L1 chain ID through the configured L1 client")?;

        let l2_chain_id: L2ChainId = env_var("EN_L2_CHAIN_ID");
        let l1_chain_id: u64 = env_var("EN_L1_CHAIN_ID");
        if l2_chain_id != remote.l2_chain_id {
            anyhow::bail!(
                "Configured L2 chain id doesn't match the one from main node.
                Make sure your configuration is correct and you are corrected to the right main node.
                Main node L2 chain id: {:?}. Local config value: {:?}",
                remote.l2_chain_id, l2_chain_id
            );
        }
        if l1_chain_id != remote.l1_chain_id.0 {
            anyhow::bail!(
                "Configured L1 chain id doesn't match the one from main node.
                Make sure your configuration is correct and you are corrected to the right main node.
                Main node L1 chain id: {}. Local config value: {}",
                remote.l1_chain_id.0, l1_chain_id
            );
        }
        if l1_chain_id != eth_chain_id.as_u64() {
            anyhow::bail!(
                "Configured L1 chain id doesn't match the one from eth node.
                Make sure your configuration is correct and you are corrected to the right eth node.
                Eth node chain id: {}. Local config value: {}",
                eth_chain_id,
                l1_chain_id
            );
        }

        let postgres = PostgresConfig::from_env()?;

        Ok(Self {
            remote,
            postgres,
            required,
            optional,
        })
    }
}

fn env_var<T>(name: &str) -> T
where
    T: std::str::FromStr,
    T::Err: std::fmt::Debug,
{
    env::var(name)
        .unwrap_or_else(|_| panic!("{} env variable is not set", name))
        .parse()
        .unwrap_or_else(|_| panic!("unable to parse {} env variable", name))
}

impl From<ExternalNodeConfig> for InternalApiConfig {
    fn from(config: ExternalNodeConfig) -> Self {
        Self {
            l1_chain_id: config.remote.l1_chain_id,
            l2_chain_id: config.remote.l2_chain_id,
            max_tx_size: config.optional.max_tx_size,
            estimate_gas_scale_factor: config.optional.estimate_gas_scale_factor,
            estimate_gas_acceptable_overestimation: config
                .optional
                .estimate_gas_acceptable_overestimation,
            bridge_addresses: BridgeAddresses {
                l1_erc20_default_bridge: config.remote.l1_erc20_bridge_proxy_addr,
                l2_erc20_default_bridge: config.remote.l2_erc20_bridge_addr,
                l1_weth_bridge: config.remote.l1_weth_bridge_proxy_addr,
                l2_weth_bridge: config.remote.l2_weth_bridge_addr,
            },
            diamond_proxy_addr: config.remote.diamond_proxy_addr,
            l2_testnet_paymaster_addr: config.remote.l2_testnet_paymaster_addr,
            req_entities_limit: config.optional.req_entities_limit,
            fee_history_limit: config.optional.fee_history_limit,
        }
    }
}

impl From<ExternalNodeConfig> for TxSenderConfig {
    fn from(config: ExternalNodeConfig) -> Self {
        Self {
            // Fee account address does not matter for the EN operation, since
            // actual fee distribution is handled my the main node.
            fee_account_addr: "0xfee0000000000000000000000000000000000000"
                .parse()
                .unwrap(),
            gas_price_scale_factor: config.optional.gas_price_scale_factor,
            max_nonce_ahead: config.optional.max_nonce_ahead,
            fair_l2_gas_price: config.remote.fair_l2_gas_price,
            vm_execution_cache_misses_limit: config.optional.vm_execution_cache_misses_limit,
            // We set these values to the maximum since we don't know the actual values
            // and they will be enforced by the main node anyway.
            max_allowed_l2_tx_gas_limit: u32::MAX,
            validation_computational_gas_limit: u32::MAX,
            chain_id: config.remote.l2_chain_id,
        }
    }
}

/// Converts the URL into a String with port provided,
/// even if it's the default one.
///
/// `url` library does not contain required functionality, yet the library we use for RPC
/// requires the port to always explicitly be set.
fn format_url_with_port(url: &Url) -> anyhow::Result<String> {
    let scheme = url.scheme();
    let host = url.host_str().context("No host in the URL")?;
    let port_str = match url.port_or_known_default() {
        Some(port) => format!(":{port}"),
        None => String::new(),
    };
    let path = url.path();
    let query_str = url.query().map(|q| format!("?{}", q)).unwrap_or_default();

    Ok(format!(
        "{scheme}://{host}{port}{path}{query_str}",
        scheme = scheme,
        host = host,
        port = port_str,
        path = path,
        query_str = query_str
    ))
}
