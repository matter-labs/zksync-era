use std::{
    env,
    ffi::OsString,
    num::{NonZeroU32, NonZeroU64, NonZeroUsize},
    time::Duration,
};

use anyhow::Context;
use serde::Deserialize;
use zksync_config::{
    configs::{
        api::{MaxResponseSize, MaxResponseSizeOverrides},
        consensus::{ConsensusConfig, ConsensusSecrets},
    },
    ObjectStoreConfig,
};
use zksync_core_leftovers::temp_config_store::decode_yaml_repr;
#[cfg(test)]
use zksync_dal::{ConnectionPool, Core};
use zksync_node_api_server::{
    tx_sender::TxSenderConfig,
    web3::{state::InternalApiConfig, Namespace},
};
use zksync_protobuf_config::proto;
use zksync_snapshots_applier::SnapshotsApplierConfig;
use zksync_types::{
    api::BridgeAddresses, commitment::L1BatchCommitmentMode, url::SensitiveUrl, Address, L1ChainId,
    L2ChainId, ETHEREUM_ADDRESS,
};
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::ClientRpcContext,
    jsonrpsee::{core::ClientError, types::error::ErrorCode},
    namespaces::{EnNamespaceClient, ZksNamespaceClient},
};

use crate::config::observability::ObservabilityENConfig;

pub(crate) mod observability;
#[cfg(test)]
mod tests;

const BYTES_IN_MEGABYTE: usize = 1_024 * 1_024;

/// Encapsulation of configuration source with a mock implementation used in tests.
trait ConfigurationSource: 'static {
    type Vars<'a>: Iterator<Item = (OsString, OsString)> + 'a;

    fn vars(&self) -> Self::Vars<'_>;

    fn var(&self, name: &str) -> Option<String>;
}

#[derive(Debug)]
struct Environment;

impl ConfigurationSource for Environment {
    type Vars<'a> = env::VarsOs;

    fn vars(&self) -> Self::Vars<'_> {
        env::vars_os()
    }

    fn var(&self, name: &str) -> Option<String> {
        env::var(name).ok()
    }
}

/// This part of the external node config is fetched directly from the main node.
#[derive(Debug, Deserialize)]
pub(crate) struct RemoteENConfig {
    pub bridgehub_proxy_addr: Option<Address>,
    pub state_transition_proxy_addr: Option<Address>,
    pub transparent_proxy_admin_addr: Option<Address>,
    pub diamond_proxy_addr: Address,
    // While on L1 shared bridge and legacy bridge are different contracts with different addresses,
    // the `l2_erc20_bridge_addr` and `l2_shared_bridge_addr` are basically the same contract, but with
    // a different name, with names adapted only for consistency.
    pub l1_shared_bridge_proxy_addr: Option<Address>,
    pub l2_shared_bridge_addr: Option<Address>,
    pub l1_erc20_bridge_proxy_addr: Option<Address>,
    pub l2_erc20_bridge_addr: Option<Address>,
    pub l1_weth_bridge_addr: Option<Address>,
    pub l2_weth_bridge_addr: Option<Address>,
    pub l2_testnet_paymaster_addr: Option<Address>,
    pub base_token_addr: Address,
    pub l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    pub dummy_verifier: bool,
}

impl RemoteENConfig {
    pub async fn fetch(client: &DynClient<L2>) -> anyhow::Result<Self> {
        let bridges = client
            .get_bridge_contracts()
            .rpc_context("get_bridge_contracts")
            .await?;
        let l2_testnet_paymaster_addr = client
            .get_testnet_paymaster()
            .rpc_context("get_testnet_paymaster")
            .await?;
        let genesis = client.genesis_config().rpc_context("genesis").await.ok();
        let ecosystem_contracts = client
            .get_ecosystem_contracts()
            .rpc_context("ecosystem_contracts")
            .await
            .ok();
        let diamond_proxy_addr = client
            .get_main_contract()
            .rpc_context("get_main_contract")
            .await?;
        let base_token_addr = match client.get_base_token_l1_address().await {
            Err(ClientError::Call(err))
                if [
                    ErrorCode::MethodNotFound.code(),
                    // This what `Web3Error::NotImplemented` gets
                    // `casted` into in the `api` server.
                    ErrorCode::InternalError.code(),
                ]
                .contains(&(err.code())) =>
            {
                // This is the fallback case for when the EN tries to interact
                // with a node that does not implement the `zks_baseTokenL1Address` endpoint.
                ETHEREUM_ADDRESS
            }
            response => response.context("Failed to fetch base token address")?,
        };

        // These two config variables should always have the same value.
        // TODO(EVM-578): double check and potentially forbid both of them being `None`.
        let l2_erc20_default_bridge = bridges
            .l2_erc20_default_bridge
            .or(bridges.l2_shared_default_bridge);
        let l2_erc20_shared_bridge = bridges
            .l2_shared_default_bridge
            .or(bridges.l2_erc20_default_bridge);

        if let (Some(legacy_addr), Some(shared_addr)) =
            (l2_erc20_default_bridge, l2_erc20_shared_bridge)
        {
            if legacy_addr != shared_addr {
                panic!("L2 erc20 bridge address and L2 shared bridge address are different.");
            }
        }

        Ok(Self {
            bridgehub_proxy_addr: ecosystem_contracts.as_ref().map(|a| a.bridgehub_proxy_addr),
            state_transition_proxy_addr: ecosystem_contracts
                .as_ref()
                .map(|a| a.state_transition_proxy_addr),
            transparent_proxy_admin_addr: ecosystem_contracts
                .as_ref()
                .map(|a| a.transparent_proxy_admin_addr),
            diamond_proxy_addr,
            l2_testnet_paymaster_addr,
            l1_erc20_bridge_proxy_addr: bridges.l1_erc20_default_bridge,
            l2_erc20_bridge_addr: l2_erc20_default_bridge,
            l1_shared_bridge_proxy_addr: bridges.l1_shared_default_bridge,
            l2_shared_bridge_addr: l2_erc20_shared_bridge,
            l1_weth_bridge_addr: bridges.l1_weth_bridge,
            l2_weth_bridge_addr: bridges.l2_weth_bridge,
            base_token_addr,
            l1_batch_commit_data_generator_mode: genesis
                .as_ref()
                .map(|a| a.l1_batch_commit_data_generator_mode)
                .unwrap_or_default(),
            dummy_verifier: genesis
                .as_ref()
                .map(|a| a.dummy_verifier)
                .unwrap_or_default(),
        })
    }

    #[cfg(test)]
    fn mock() -> Self {
        Self {
            bridgehub_proxy_addr: None,
            state_transition_proxy_addr: None,
            transparent_proxy_admin_addr: None,
            diamond_proxy_addr: Address::repeat_byte(1),
            l1_erc20_bridge_proxy_addr: Some(Address::repeat_byte(2)),
            l2_erc20_bridge_addr: Some(Address::repeat_byte(3)),
            l2_weth_bridge_addr: None,
            l2_testnet_paymaster_addr: None,
            base_token_addr: Address::repeat_byte(4),
            l1_shared_bridge_proxy_addr: Some(Address::repeat_byte(5)),
            l1_weth_bridge_addr: None,
            l2_shared_bridge_addr: Some(Address::repeat_byte(6)),
            l1_batch_commit_data_generator_mode: L1BatchCommitmentMode::Rollup,
            dummy_verifier: true,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) enum BlockFetcher {
    ServerAPI,
    Consensus,
}

/// This part of the external node config is completely optional to provide.
/// It can tweak limits of the API, delay intervals of certain components, etc.
/// If any of the fields are not provided, the default values will be used.
#[derive(Debug, Deserialize)]
pub(crate) struct OptionalENConfig {
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
    /// Max possible size of an ABI-encoded transaction supplied to `eth_sendRawTransaction`.
    #[serde(
        alias = "max_tx_size",
        default = "OptionalENConfig::default_max_tx_size_bytes"
    )]
    pub max_tx_size_bytes: usize,
    /// Max number of cache misses during one VM execution. If the number of cache misses exceeds this value, the API server panics.
    /// This is a temporary solution to mitigate API request resulting in thousands of DB queries.
    pub vm_execution_cache_misses_limit: Option<usize>,
    /// Note: Deprecated option, no longer in use. Left to display a warning in case someone used them.
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
    /// Method-specific overrides in MiBs for the maximum response body size.
    #[serde(default = "MaxResponseSizeOverrides::empty")]
    max_response_body_size_overrides_mb: MaxResponseSizeOverrides,

    // Other API config settings
    /// Interval between polling DB for Web3 subscriptions.
    #[serde(
        alias = "pubsub_polling_interval",
        default = "OptionalENConfig::default_polling_interval"
    )]
    pubsub_polling_interval_ms: u64,
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
    /// Polling period for mempool cache update - how often the mempool cache is updated from the database.
    /// Default is 50 milliseconds.
    #[serde(
        alias = "mempool_cache_update_interval",
        default = "OptionalENConfig::default_mempool_cache_update_interval_ms"
    )]
    pub mempool_cache_update_interval_ms: u64,
    /// Maximum number of transactions to be stored in the mempool cache.
    #[serde(default = "OptionalENConfig::default_mempool_cache_size")]
    pub mempool_cache_size: usize,
    /// Enables extended tracing of RPC calls. This may negatively impact performance for nodes under high load
    /// (hundreds or thousands RPS).
    #[serde(default = "OptionalENConfig::default_extended_api_tracing")]
    pub extended_rpc_tracing: bool,

    // Health checks
    /// Time limit in milliseconds to mark a health check as slow and log the corresponding warning.
    /// If not specified, the default value in the health check crate will be used.
    healthcheck_slow_time_limit_ms: Option<u64>,
    /// Time limit in milliseconds to abort a health check and return "not ready" status for the corresponding component.
    /// If not specified, the default value in the health check crate will be used.
    healthcheck_hard_time_limit_ms: Option<u64>,

    // Gas estimation config
    /// The factor by which to scale the gas limit.
    #[serde(default = "OptionalENConfig::default_estimate_gas_scale_factor")]
    pub estimate_gas_scale_factor: f64,
    /// The max possible number of gas that `eth_estimateGas` is allowed to overestimate.
    #[serde(default = "OptionalENConfig::default_estimate_gas_acceptable_overestimation")]
    pub estimate_gas_acceptable_overestimation: u32,
    /// The multiplier to use when suggesting gas price. Should be higher than one,
    /// otherwise if the L1 prices soar, the suggested gas price won't be sufficient to be included in block.
    #[serde(default = "OptionalENConfig::default_gas_price_scale_factor")]
    pub gas_price_scale_factor: f64,

    // Merkle tree config
    /// Processing delay between processing L1 batches in the Merkle tree.
    #[serde(
        alias = "metadata_calculator_delay",
        default = "OptionalENConfig::default_merkle_tree_processing_delay_ms"
    )]
    merkle_tree_processing_delay_ms: u64,
    /// Maximum number of L1 batches to be processed by the Merkle tree at a time. L1 batches are processed in a bulk
    /// only if they are readily available (i.e., mostly during node catch-up). Increasing this value reduces the number
    /// of I/O operations at the cost of requiring more RAM (order of 100 MB / batch).
    #[serde(
        alias = "max_blocks_per_tree_batch",
        alias = "max_l1_batches_per_tree_iter",
        default = "OptionalENConfig::default_merkle_tree_max_l1_batches_per_iter"
    )]
    pub merkle_tree_max_l1_batches_per_iter: usize,
    /// Maximum number of files concurrently opened by Merkle tree RocksDB. Useful to fit into OS limits; can be used
    /// as a rudimentary way to control RAM usage of the tree.
    pub merkle_tree_max_open_files: Option<NonZeroU32>,
    /// Chunk size for multi-get operations. Can speed up loading data for the Merkle tree on some environments,
    /// but the effects vary wildly depending on the setup (e.g., the filesystem used).
    #[serde(default = "OptionalENConfig::default_merkle_tree_multi_get_chunk_size")]
    pub merkle_tree_multi_get_chunk_size: usize,
    /// Capacity of the block cache for the Merkle tree RocksDB. Reasonable values range from ~100 MiB to several GiB.
    /// The default value is 128 MiB.
    #[serde(default = "OptionalENConfig::default_merkle_tree_block_cache_size_mb")]
    merkle_tree_block_cache_size_mb: usize,
    /// If specified, RocksDB indices and Bloom filters will be managed by the block cache, rather than
    /// being loaded entirely into RAM on the RocksDB initialization. The block cache capacity should be increased
    /// correspondingly; otherwise, RocksDB performance can significantly degrade.
    #[serde(default)]
    pub merkle_tree_include_indices_and_filters_in_block_cache: bool,
    /// Byte capacity of memtables (recent, non-persisted changes to RocksDB). Setting this to a reasonably
    /// large value (order of 512 MiB) is helpful for large DBs that experience write stalls.
    #[serde(default = "OptionalENConfig::default_merkle_tree_memtable_capacity_mb")]
    merkle_tree_memtable_capacity_mb: usize,
    /// Timeout to wait for the Merkle tree database to run compaction on stalled writes.
    #[serde(default = "OptionalENConfig::default_merkle_tree_stalled_writes_timeout_sec")]
    merkle_tree_stalled_writes_timeout_sec: u64,

    // Postgres config (new parameters)
    /// Threshold in milliseconds for the DB connection lifetime to denote it as long-living and log its details.
    /// If not specified, such logging will be disabled.
    database_long_connection_threshold_ms: Option<u64>,
    /// Threshold in milliseconds to denote a DB query as "slow" and log its details. If not specified, such logging will be disabled.
    database_slow_query_threshold_ms: Option<u64>,

    // Other config settings
    /// Capacity of the queue for asynchronous L2 block sealing. Once this many L2 blocks are queued,
    /// sealing will block until some of the L2 blocks from the queue are processed.
    /// 0 means that sealing is synchronous; this is mostly useful for performance comparison, testing etc.
    #[serde(
        alias = "miniblock_seal_queue_capacity",
        default = "OptionalENConfig::default_l2_block_seal_queue_capacity"
    )]
    pub l2_block_seal_queue_capacity: usize,
    /// Configures whether to persist protective reads when persisting L1 batches in the state keeper.
    /// Protective reads are never required by full nodes so far, not until such a node runs a full Merkle tree
    /// (presumably, to participate in L1 batch proving).
    /// By default, set to `true` as a temporary safety measure.
    #[serde(default = "OptionalENConfig::default_protective_reads_persistence_enabled")]
    pub protective_reads_persistence_enabled: bool,
    /// Address of the L1 diamond proxy contract used by the consistency checker to match with the origin of logs emitted
    /// by commit transactions. If not set, it will not be verified.
    // This is intentionally not a part of `RemoteENConfig` because fetching this info from the main node would defeat
    // its purpose; the consistency checker assumes that the main node may provide false information.
    pub contracts_diamond_proxy_addr: Option<Address>,
    /// Number of requests per second allocated for the main node HTTP client. Default is 100 requests.
    #[serde(default = "OptionalENConfig::default_main_node_rate_limit_rps")]
    pub main_node_rate_limit_rps: NonZeroUsize,

    #[serde(default)]
    pub l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    /// Enables application-level snapshot recovery. Required to start a node that was recovered from a snapshot,
    /// or to initialize a node from a snapshot. Has no effect if a node that was initialized from a Postgres dump
    /// or was synced from genesis.
    ///
    /// This is an experimental and incomplete feature; do not use unless you know what you're doing.
    #[serde(default)]
    pub snapshots_recovery_enabled: bool,
    /// Maximum concurrency factor for the concurrent parts of snapshot recovery for Postgres. It may be useful to
    /// reduce this factor to about 5 if snapshot recovery overloads I/O capacity of the node. Conversely,
    /// if I/O capacity of your infra is high, you may increase concurrency to speed up Postgres recovery.
    #[serde(default = "OptionalENConfig::default_snapshots_recovery_postgres_max_concurrency")]
    pub snapshots_recovery_postgres_max_concurrency: NonZeroUsize,

    /// Enables pruning of the historical node state (Postgres and Merkle tree). The node will retain
    /// recent state and will continuously remove (prune) old enough parts of the state in the background.
    #[serde(default)]
    pub pruning_enabled: bool,
    /// Number of L1 batches pruned at a time.
    #[serde(default = "OptionalENConfig::default_pruning_chunk_size")]
    pub pruning_chunk_size: u32,
    /// Delta between soft- and hard-removing data from Postgres. Should be reasonably large (order of 60 seconds).
    /// The default value is 60 seconds.
    #[serde(default = "OptionalENConfig::default_pruning_removal_delay_sec")]
    pruning_removal_delay_sec: NonZeroU64,
    /// If set, L1 batches will be pruned after the batch timestamp is this old (in seconds). Note that an L1 batch
    /// may be temporarily retained for other reasons; e.g., a batch cannot be pruned until it is executed on L1,
    /// which happens roughly 24 hours after its generation on the mainnet. Thus, in practice this value can specify
    /// the retention period greater than that implicitly imposed by other criteria (e.g., 7 or 30 days).
    /// If set to 0, L1 batches will not be retained based on their timestamp. The default value is 1 hour.
    #[serde(default = "OptionalENConfig::default_pruning_data_retention_sec")]
    pruning_data_retention_sec: u64,
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

    const fn default_max_tx_size_bytes() -> usize {
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

    const fn default_merkle_tree_processing_delay_ms() -> u64 {
        100
    }

    const fn default_merkle_tree_max_l1_batches_per_iter() -> usize {
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

    const fn default_l2_block_seal_queue_capacity() -> usize {
        10
    }

    const fn default_protective_reads_persistence_enabled() -> bool {
        true
    }

    const fn default_mempool_cache_update_interval_ms() -> u64 {
        50
    }

    const fn default_mempool_cache_size() -> usize {
        10_000
    }

    const fn default_extended_api_tracing() -> bool {
        true
    }

    fn default_main_node_rate_limit_rps() -> NonZeroUsize {
        NonZeroUsize::new(100).unwrap()
    }

    fn default_snapshots_recovery_postgres_max_concurrency() -> NonZeroUsize {
        SnapshotsApplierConfig::default().max_concurrency
    }

    const fn default_pruning_chunk_size() -> u32 {
        10
    }

    fn default_pruning_removal_delay_sec() -> NonZeroU64 {
        NonZeroU64::new(60).unwrap()
    }

    fn default_pruning_data_retention_sec() -> u64 {
        3_600 // 1 hour
    }

    fn from_env() -> anyhow::Result<Self> {
        envy::prefixed("EN_")
            .from_env()
            .context("could not load external node config")
    }

    pub fn polling_interval(&self) -> Duration {
        Duration::from_millis(self.pubsub_polling_interval_ms)
    }

    pub fn merkle_tree_processing_delay(&self) -> Duration {
        Duration::from_millis(self.merkle_tree_processing_delay_ms)
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

    pub fn long_connection_threshold(&self) -> Option<Duration> {
        self.database_long_connection_threshold_ms
            .map(Duration::from_millis)
    }

    pub fn slow_query_threshold(&self) -> Option<Duration> {
        self.database_slow_query_threshold_ms
            .map(Duration::from_millis)
    }

    pub fn api_namespaces(&self) -> Vec<Namespace> {
        self.api_namespaces
            .clone()
            .unwrap_or_else(|| Namespace::DEFAULT.to_vec())
    }

    pub fn max_response_body_size(&self) -> MaxResponseSize {
        let scale = NonZeroUsize::new(BYTES_IN_MEGABYTE).unwrap();
        MaxResponseSize {
            global: self.max_response_body_size_mb * BYTES_IN_MEGABYTE,
            overrides: self.max_response_body_size_overrides_mb.scale(scale),
        }
    }

    pub fn healthcheck_slow_time_limit(&self) -> Option<Duration> {
        self.healthcheck_slow_time_limit_ms
            .map(Duration::from_millis)
    }

    pub fn healthcheck_hard_time_limit(&self) -> Option<Duration> {
        self.healthcheck_hard_time_limit_ms
            .map(Duration::from_millis)
    }

    pub fn mempool_cache_update_interval(&self) -> Duration {
        Duration::from_millis(self.mempool_cache_update_interval_ms)
    }

    pub fn pruning_removal_delay(&self) -> Duration {
        Duration::from_secs(self.pruning_removal_delay_sec.get())
    }

    pub fn pruning_data_retention(&self) -> Duration {
        Duration::from_secs(self.pruning_data_retention_sec)
    }

    #[cfg(test)]
    fn mock() -> Self {
        // Set all values to their defaults
        serde_json::from_str("{}").unwrap()
    }
}

/// This part of the external node config is required for its operation.
#[derive(Debug, Deserialize)]
pub(crate) struct RequiredENConfig {
    /// L1 chain ID (e.g., 9 for Ethereum mainnet). This ID will be checked against the `eth_client_url` RPC provider on initialization
    /// to ensure that there's no mismatch between the expected and actual L1 network.
    pub l1_chain_id: L1ChainId,
    /// L2 chain ID (e.g., 270 for zkSync Era mainnet). This ID will be checked against the `main_node_url` RPC provider on initialization
    /// to ensure that there's no mismatch between the expected and actual L2 network.
    pub l2_chain_id: L2ChainId,

    /// Port on which the HTTP RPC server is listening.
    pub http_port: u16,
    /// Port on which the WebSocket RPC server is listening.
    pub ws_port: u16,
    /// Port on which the healthcheck REST server is listening.
    pub healthcheck_port: u16,
    /// Address of the Ethereum node API.
    pub eth_client_url: SensitiveUrl,
    /// Main node URL - used by external node to proxy transactions to, query state from, etc.
    pub main_node_url: SensitiveUrl,
    /// Path to the database data directory that serves state cache.
    pub state_cache_path: String,
    /// Fast SSD path. Used as a RocksDB dir for the Merkle tree (*new* implementation).
    pub merkle_tree_path: String,
}

impl RequiredENConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy::prefixed("EN_")
            .from_env()
            .context("could not load external node config")
    }

    #[cfg(test)]
    fn mock(temp_dir: &tempfile::TempDir) -> Self {
        Self {
            l1_chain_id: L1ChainId(9),
            l2_chain_id: L2ChainId::default(),
            http_port: 0,
            ws_port: 0,
            healthcheck_port: 0,
            // L1 and L2 clients must be instantiated before accessing mocks, so these values don't matter
            eth_client_url: "http://localhost".parse().unwrap(),
            main_node_url: "http://localhost".parse().unwrap(),
            state_cache_path: temp_dir
                .path()
                .join("state_keeper_cache")
                .to_str()
                .unwrap()
                .to_owned(),
            merkle_tree_path: temp_dir.path().join("tree").to_str().unwrap().to_owned(),
        }
    }
}

/// Configuration for Postgres database.
/// While also mandatory, it historically used different naming scheme for corresponding
/// environment variables.
/// Thus it is kept separately for backward compatibility and ease of deserialization.
#[derive(Debug, Deserialize)]
pub(crate) struct PostgresConfig {
    database_url: SensitiveUrl,
    pub max_connections: u32,
}

impl PostgresConfig {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: env::var("DATABASE_URL")
                .context("DATABASE_URL env variable is not set")?
                .parse()
                .context("DATABASE_URL env variable is not a valid Postgres URL")?,
            max_connections: env::var("DATABASE_POOL_SIZE")
                .context("DATABASE_POOL_SIZE env variable is not set")?
                .parse()
                .context("Unable to parse DATABASE_POOL_SIZE env variable")?,
        })
    }

    pub fn database_url(&self) -> SensitiveUrl {
        self.database_url.clone()
    }

    #[cfg(test)]
    fn mock(test_pool: &ConnectionPool<Core>) -> Self {
        Self {
            database_url: test_pool.database_url().clone(),
            max_connections: test_pool.max_size(),
        }
    }
}

/// Experimental part of the external node config. All parameters in this group can change or disappear without notice.
/// Eventually, parameters from this group generally end up in the optional group.
#[derive(Debug, Deserialize)]
pub(crate) struct ExperimentalENConfig {
    // State keeper cache config
    /// Block cache capacity of the state keeper RocksDB cache. The default value is 128 MB.
    #[serde(default = "ExperimentalENConfig::default_state_keeper_db_block_cache_capacity_mb")]
    state_keeper_db_block_cache_capacity_mb: usize,
    /// Maximum number of files concurrently opened by state keeper cache RocksDB. Useful to fit into OS limits; can be used
    /// as a rudimentary way to control RAM usage of the cache.
    pub state_keeper_db_max_open_files: Option<NonZeroU32>,

    // Commitment generator
    /// Maximum degree of parallelism during commitment generation, i.e., the maximum number of L1 batches being processed in parallel.
    /// If not specified, commitment generator will use a value roughly equal to the number of CPU cores with some clamping applied.
    pub commitment_generator_max_parallelism: Option<NonZeroU32>,
}

impl ExperimentalENConfig {
    const fn default_state_keeper_db_block_cache_capacity_mb() -> usize {
        128
    }

    #[cfg(test)]
    fn mock() -> Self {
        Self {
            state_keeper_db_block_cache_capacity_mb:
                Self::default_state_keeper_db_block_cache_capacity_mb(),
            state_keeper_db_max_open_files: None,
            commitment_generator_max_parallelism: None,
        }
    }

    /// Returns the size of block cache for the state keeper RocksDB cache in bytes.
    pub fn state_keeper_db_block_cache_capacity(&self) -> usize {
        self.state_keeper_db_block_cache_capacity_mb * BYTES_IN_MEGABYTE
    }
}

pub(crate) fn read_consensus_secrets() -> anyhow::Result<Option<ConsensusSecrets>> {
    let Ok(path) = env::var("EN_CONSENSUS_SECRETS_PATH") else {
        return Ok(None);
    };
    let cfg = std::fs::read_to_string(&path).context(path)?;
    Ok(Some(
        decode_yaml_repr::<proto::secrets::ConsensusSecrets>(&cfg)
            .context("failed decoding YAML")?,
    ))
}

pub(crate) fn read_consensus_config() -> anyhow::Result<Option<ConsensusConfig>> {
    let Ok(path) = env::var("EN_CONSENSUS_CONFIG_PATH") else {
        return Ok(None);
    };
    let cfg = std::fs::read_to_string(&path).context(path)?;
    Ok(Some(
        decode_yaml_repr::<proto::consensus::Config>(&cfg).context("failed decoding YAML")?,
    ))
}

/// Configuration for snapshot recovery. Loaded optionally, only if snapshot recovery is enabled.
#[derive(Debug)]
pub(crate) struct SnapshotsRecoveryConfig {
    pub snapshots_object_store: ObjectStoreConfig,
}

impl SnapshotsRecoveryConfig {
    pub fn new() -> anyhow::Result<Self> {
        let snapshots_object_store = envy::prefixed("EN_SNAPSHOTS_OBJECT_STORE_")
            .from_env::<ObjectStoreConfig>()
            .context("failed loading snapshot object store config from env variables")?;
        Ok(Self {
            snapshots_object_store,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct ApiComponentConfig {
    /// Address of the tree API used by this EN in case it does not have a
    /// local tree component running and in this case needs to send requests
    /// to some external tree API.
    pub tree_api_remote_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TreeComponentConfig {
    pub api_port: Option<u16>,
}

/// External Node Config contains all the configuration required for the EN operation.
/// It is split into three parts: required, optional and remote for easier navigation.
#[derive(Debug)]
pub(crate) struct ExternalNodeConfig<R = RemoteENConfig> {
    pub required: RequiredENConfig,
    pub postgres: PostgresConfig,
    pub optional: OptionalENConfig,
    pub observability: ObservabilityENConfig,
    pub experimental: ExperimentalENConfig,
    pub consensus: Option<ConsensusConfig>,
    pub api_component: ApiComponentConfig,
    pub tree_component: TreeComponentConfig,
    pub remote: R,
}

impl ExternalNodeConfig<()> {
    /// Parses the local part of node configuration from the environment.
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            required: RequiredENConfig::from_env()?,
            postgres: PostgresConfig::from_env()?,
            optional: OptionalENConfig::from_env()?,
            observability: ObservabilityENConfig::from_env()?,
            experimental: envy::prefixed("EN_EXPERIMENTAL_")
                .from_env::<ExperimentalENConfig>()
                .context("could not load external node config (experimental params)")?,
            consensus: read_consensus_config().context("read_consensus_config()")?,
            api_component: envy::prefixed("EN_API_")
                .from_env::<ApiComponentConfig>()
                .context("could not load external node config (API component params)")?,
            tree_component: envy::prefixed("EN_TREE_")
                .from_env::<TreeComponentConfig>()
                .context("could not load external node config (tree component params)")?,
            remote: (),
        })
    }

    /// Fetches contracts addresses from the main node, completing the configuration.
    pub async fn fetch_remote(
        self,
        main_node_client: &DynClient<L2>,
    ) -> anyhow::Result<ExternalNodeConfig> {
        let remote = RemoteENConfig::fetch(main_node_client)
            .await
            .context("Unable to fetch required config values from the main node")?;
        Ok(ExternalNodeConfig {
            required: self.required,
            postgres: self.postgres,
            optional: self.optional,
            observability: self.observability,
            experimental: self.experimental,
            consensus: self.consensus,
            tree_component: self.tree_component,
            api_component: self.api_component,
            remote,
        })
    }
}

impl ExternalNodeConfig {
    #[cfg(test)]
    pub(crate) fn mock(temp_dir: &tempfile::TempDir, test_pool: &ConnectionPool<Core>) -> Self {
        Self {
            required: RequiredENConfig::mock(temp_dir),
            postgres: PostgresConfig::mock(test_pool),
            optional: OptionalENConfig::mock(),
            remote: RemoteENConfig::mock(),
            observability: ObservabilityENConfig::default(),
            experimental: ExperimentalENConfig::mock(),
            consensus: None,
            api_component: ApiComponentConfig {
                tree_api_remote_url: None,
            },
            tree_component: TreeComponentConfig { api_port: None },
        }
    }
}

impl From<&ExternalNodeConfig> for InternalApiConfig {
    fn from(config: &ExternalNodeConfig) -> Self {
        Self {
            l1_chain_id: config.required.l1_chain_id,
            l2_chain_id: config.required.l2_chain_id,
            max_tx_size: config.optional.max_tx_size_bytes,
            estimate_gas_scale_factor: config.optional.estimate_gas_scale_factor,
            estimate_gas_acceptable_overestimation: config
                .optional
                .estimate_gas_acceptable_overestimation,
            bridge_addresses: BridgeAddresses {
                l1_erc20_default_bridge: config.remote.l1_erc20_bridge_proxy_addr,
                l2_erc20_default_bridge: config.remote.l2_erc20_bridge_addr,
                l1_shared_default_bridge: config.remote.l1_shared_bridge_proxy_addr,
                l2_shared_default_bridge: config.remote.l2_shared_bridge_addr,
                l1_weth_bridge: config.remote.l1_weth_bridge_addr,
                l2_weth_bridge: config.remote.l2_weth_bridge_addr,
            },
            bridgehub_proxy_addr: config.remote.bridgehub_proxy_addr,
            state_transition_proxy_addr: config.remote.state_transition_proxy_addr,
            transparent_proxy_admin_addr: config.remote.transparent_proxy_admin_addr,
            diamond_proxy_addr: config.remote.diamond_proxy_addr,
            l2_testnet_paymaster_addr: config.remote.l2_testnet_paymaster_addr,
            req_entities_limit: config.optional.req_entities_limit,
            fee_history_limit: config.optional.fee_history_limit,
            base_token_address: Some(config.remote.base_token_addr),
            filters_disabled: config.optional.filters_disabled,
            dummy_verifier: config.remote.dummy_verifier,
            l1_batch_commit_data_generator_mode: config.remote.l1_batch_commit_data_generator_mode,
        }
    }
}

impl From<&ExternalNodeConfig> for TxSenderConfig {
    fn from(config: &ExternalNodeConfig) -> Self {
        Self {
            // Fee account address does not matter for the EN operation, since
            // actual fee distribution is handled my the main node.
            fee_account_addr: "0xfee0000000000000000000000000000000000000"
                .parse()
                .unwrap(),
            gas_price_scale_factor: config.optional.gas_price_scale_factor,
            max_nonce_ahead: config.optional.max_nonce_ahead,
            vm_execution_cache_misses_limit: config.optional.vm_execution_cache_misses_limit,
            // We set these values to the maximum since we don't know the actual values
            // and they will be enforced by the main node anyway.
            max_allowed_l2_tx_gas_limit: u64::MAX,
            validation_computational_gas_limit: u32::MAX,
            chain_id: config.required.l2_chain_id,
            // Does not matter for EN.
            whitelisted_tokens_for_aa: Default::default(),
        }
    }
}
