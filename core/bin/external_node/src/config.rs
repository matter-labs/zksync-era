use anyhow::Context;
use serde::Deserialize;
use std::{env, time::Duration};
use url::Url;

use zksync_basic_types::{Address, L1ChainId, L2ChainId, H256};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_core::api_server::{tx_sender::TxSenderConfig, web3::state::InternalApiConfig};
use zksync_types::api::BridgeAddresses;

use zksync_web3_decl::{
    jsonrpsee::http_client::{HttpClient, HttpClientBuilder},
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};

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
        let l2_chain_id = L2ChainId(
            client
                .chain_id()
                .await
                .context("Failed to fetch L2 chain ID")?
                .as_u64() as u16,
        );
        let l1_chain_id = L1ChainId(
            client
                .l1_chain_id()
                .await
                .context("Failed to fetch L1 chain ID")?
                .as_u64(),
        );

        Ok(Self {
            diamond_proxy_addr,
            l2_testnet_paymaster_addr,
            l1_erc20_bridge_proxy_addr: bridges.l1_erc20_default_bridge,
            l2_erc20_bridge_addr: bridges.l2_erc20_default_bridge,
            l1_weth_bridge_proxy_addr: bridges.l1_weth_bridge,
            l2_weth_bridge_addr: bridges.l2_weth_bridge,
            l2_chain_id,
            l1_chain_id,
        })
    }
}

/// This part of the external node config is completely optional to provide.
/// It can tweak limits of the API, delay intervals of cetrain components, etc.
/// If any of the fields are not provided, the default values will be used.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct OptionalENConfig {
    /// Max possible limit of filters to be in the API state at once.
    filters_limit: Option<usize>,
    /// Max possible limit of subscriptions to be in the API state at once.
    subscriptions_limit: Option<usize>,
    /// Interval between polling db for pubsub (in ms).
    pubsub_polling_interval: Option<u64>,
    /// Max possible limit of entities to be requested via API at once.
    req_entities_limit: Option<usize>,
    ///  Max possible size of an ABI encoded tx (in bytes).
    max_tx_size: Option<usize>,
    /// The factor by which to scale the gasLimit
    estimate_gas_scale_factor: Option<f64>,
    /// The max possible number of gas that `eth_estimateGas` is allowed to overestimate.
    estimate_gas_acceptable_overestimation: Option<u32>,
    /// The multiplier to use when suggesting gas price. Should be higher than one,
    /// otherwise if the L1 prices soar, the suggested gas price won't be sufficient to be included in block
    gas_price_scale_factor: Option<f64>,
    /// Tx nonce: how far ahead from the committed nonce can it be.
    max_nonce_ahead: Option<u32>,
    metadata_calculator_delay: Option<u64>,
    /// Max number of cache misses during one VM execution. If the number of cache misses exceeds this value, the api server panics.
    /// This is a temporary solution to mitigate API request resulting in thousands of DB queries.
    pub vm_execution_cache_misses_limit: Option<usize>,
    /// Inbound transaction limit used for throttling.
    pub transactions_per_sec_limit: Option<u32>,
    /// Port on which the Prometheus exporter server is listening.
    pub prometheus_port: Option<u16>,
    /// Throttle interval for the tree in milliseconds. This interval will be
    /// applied after each time the tree makes progress.
    merkle_tree_throttle: Option<u64>,
    /// Maximum number of blocks to be processed by the Merkle tree at a time.
    max_blocks_per_tree_batch: Option<usize>,
    /// Max number of VM instances to be concurrently spawned by the API server.
    /// This option can be tweaked down if the API server is running out of memory.
    vm_concurrency_limit: Option<usize>,
    /// Smart contract source code cache size for the API server.
    factory_deps_cache_size_mb: Option<usize>,
}

impl OptionalENConfig {
    pub fn polling_interval(&self) -> Duration {
        Duration::from_millis(self.pubsub_polling_interval.unwrap_or(200))
    }

    pub fn req_entities_limit(&self) -> usize {
        self.req_entities_limit.unwrap_or(1024)
    }

    pub fn filters_limit(&self) -> usize {
        self.filters_limit.unwrap_or(10000)
    }

    pub fn subscriptions_limit(&self) -> usize {
        self.subscriptions_limit.unwrap_or(10000)
    }

    pub fn max_tx_size(&self) -> usize {
        self.max_tx_size.unwrap_or(1000000)
    }

    pub fn estimate_gas_scale_factor(&self) -> f64 {
        self.estimate_gas_scale_factor.unwrap_or(1.2)
    }

    pub fn estimate_gas_acceptable_overestimation(&self) -> u32 {
        self.estimate_gas_acceptable_overestimation.unwrap_or(1000)
    }

    pub fn gas_price_scale_factor(&self) -> f64 {
        self.gas_price_scale_factor.unwrap_or(1.2)
    }

    pub fn max_nonce_ahead(&self) -> u32 {
        self.max_nonce_ahead.unwrap_or(50)
    }

    pub fn metadata_calculator_delay(&self) -> Duration {
        Duration::from_millis(self.metadata_calculator_delay.unwrap_or(100))
    }

    pub fn max_blocks_per_tree_batch(&self) -> usize {
        self.max_blocks_per_tree_batch.unwrap_or(100)
    }

    pub fn merkle_tree_throttle(&self) -> Duration {
        Duration::from_millis(self.merkle_tree_throttle.unwrap_or(0))
    }

    pub fn vm_concurrency_limit(&self) -> Option<usize> {
        self.vm_concurrency_limit
    }

    pub fn factory_deps_cache_size_mb(&self) -> usize {
        // 128MB is the default smart contract code cache size.
        self.factory_deps_cache_size_mb.unwrap_or(128)
    }
}

/// This part of the external node config is required for its operation.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct RequiredENConfig {
    /// Default AA hash used at genesis.
    pub default_aa_hash: H256,
    /// Bootloader hash used at genesis.
    pub bootloader_hash: H256,

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

    pub max_allowed_l2_tx_gas_limit: u32,
    pub fee_account_addr: Address,
    pub fair_l2_gas_price: u64,
    pub validation_computational_gas_limit: u32,
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

/// External Node Config contains all the configuration required for the EN operation.
/// It is split into three parts: required, optional and remote for easier navigation.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ExternalNodeConfig {
    pub required: RequiredENConfig,
    pub optional: OptionalENConfig,
    pub remote: RemoteENConfig,
}

impl ExternalNodeConfig {
    pub fn base_system_contracts_hashes(&self) -> BaseSystemContractsHashes {
        BaseSystemContractsHashes {
            bootloader: self.required.bootloader_hash,
            default_aa: self.required.default_aa_hash,
        }
    }

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

        let l2_chain_id: u16 = env_var("EN_L2_CHAIN_ID");
        let l1_chain_id: u64 = env_var("EN_L1_CHAIN_ID");
        if l2_chain_id != remote.l2_chain_id.0 {
            anyhow::bail!(
                "Configured L2 chain id doesn't match the one from main node.
                Make sure your configuration is correct and you are corrected to the right main node.
                Main node L2 chain id: {}. Local config value: {}",
                remote.l2_chain_id.0, l2_chain_id
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

        Ok(Self {
            remote,
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
            max_tx_size: config.optional.max_tx_size(),
            estimate_gas_scale_factor: config.optional.estimate_gas_scale_factor(),
            estimate_gas_acceptable_overestimation: config
                .optional
                .estimate_gas_acceptable_overestimation(),
            bridge_addresses: BridgeAddresses {
                l1_erc20_default_bridge: config.remote.l1_erc20_bridge_proxy_addr,
                l2_erc20_default_bridge: config.remote.l2_erc20_bridge_addr,
                l1_weth_bridge: config.remote.l1_weth_bridge_proxy_addr,
                l2_weth_bridge: config.remote.l2_weth_bridge_addr,
            },
            diamond_proxy_addr: config.remote.diamond_proxy_addr,
            l2_testnet_paymaster_addr: config.remote.l2_testnet_paymaster_addr,
            req_entities_limit: config.optional.req_entities_limit(),
        }
    }
}

impl From<ExternalNodeConfig> for TxSenderConfig {
    fn from(config: ExternalNodeConfig) -> Self {
        Self {
            fee_account_addr: config.required.fee_account_addr,
            gas_price_scale_factor: config.optional.gas_price_scale_factor(),
            max_nonce_ahead: config.optional.max_nonce_ahead(),
            max_allowed_l2_tx_gas_limit: config.required.max_allowed_l2_tx_gas_limit,
            fair_l2_gas_price: config.required.fair_l2_gas_price,
            vm_execution_cache_misses_limit: config.optional.vm_execution_cache_misses_limit,
            validation_computational_gas_limit: config.required.validation_computational_gas_limit,
            default_aa: config.required.default_aa_hash,
            bootloader: config.required.bootloader_hash,
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
