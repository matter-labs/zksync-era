/// External uses
use serde::Deserialize;
use std::str::FromStr;
/// Built-in uses
use std::time::Duration;
// Local uses
use zksync_basic_types::network::Network;
use zksync_basic_types::{Address, L2ChainId};

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ChainConfig {
    /// L1 parameters configuration.
    pub network: NetworkConfig,
    /// State keeper / block generating configuration.
    pub state_keeper: StateKeeperConfig,
    /// Operations manager / Metadata calculator.
    pub operations_manager: OperationsManagerConfig,
    /// mempool configuration
    pub mempool: MempoolConfig,
    /// circuit breaker configuration
    pub circuit_breaker: CircuitBreakerConfig,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct NetworkConfig {
    /// Name of the used Ethereum network, e.g. `localhost` or `rinkeby`.
    pub network: Network,
    /// Name of current zkSync network
    /// Used for Sentry environment
    pub zksync_network: String,
    /// ID of current zkSync network treated as ETH network ID.
    /// Used to distinguish zkSync from other Web3-capable networks.
    pub zksync_network_id: L2ChainId,
}

impl NetworkConfig {
    /// Creates a config object suitable for use in unit tests.
    pub fn for_tests() -> NetworkConfig {
        Self {
            network: Network::Localhost,
            zksync_network: "localhost".into(),
            zksync_network_id: L2ChainId::default(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
pub struct StateKeeperConfig {
    /// The max number of slots for txs in a block before it should be sealed by the slots sealer.
    pub transaction_slots: usize,

    /// Number of ms after which an L1 batch is going to be unconditionally sealed.
    pub block_commit_deadline_ms: u64,
    /// Number of ms after which a miniblock should be sealed by the timeout sealer.
    pub miniblock_commit_deadline_ms: u64,
    /// Capacity of the queue for asynchronous miniblock sealing. Once this many miniblocks are queued,
    /// sealing will block until some of the miniblocks from the queue are processed.
    /// 0 means that sealing is synchronous; this is mostly useful for performance comparison, testing etc.
    pub miniblock_seal_queue_capacity: usize,

    /// The max number of gas to spend on an L1 tx before its batch should be sealed by the gas sealer.
    pub max_single_tx_gas: u32,

    pub max_allowed_l2_tx_gas_limit: u32,

    /// Configuration option for tx to be rejected in case
    /// it takes more percentage of the block capacity than this value.
    pub reject_tx_at_geometry_percentage: f64,
    /// Configuration option for tx to be rejected in case
    /// it takes more percentage of the block capacity than this value.
    pub reject_tx_at_eth_params_percentage: f64,
    /// Configuration option for tx to be rejected in case
    /// it takes more percentage of the block capacity than this value.
    pub reject_tx_at_gas_percentage: f64,
    /// Denotes the percentage of geometry params used in L2 block that triggers L2 block seal.
    pub close_block_at_geometry_percentage: f64,
    /// Denotes the percentage of L1 params used in L2 block that triggers L2 block seal.
    pub close_block_at_eth_params_percentage: f64,
    /// Denotes the percentage of L1 gas used in l2 block that triggers L2 block seal.
    pub close_block_at_gas_percentage: f64,

    pub fee_account_addr: Address,

    /// The price the operator spends on 1 gas of computation in wei.
    pub fair_l2_gas_price: u64,

    /// Max number of computational gas that validation step is allowed to take.
    pub validation_computational_gas_limit: u32,
    pub save_call_traces: bool,

    pub virtual_blocks_interval: u32,
    pub virtual_blocks_per_miniblock: u32,

    /// Flag which will enable storage to cache witness_inputs during State Keeper's run.
    /// NOTE: This will slow down StateKeeper, to be used in non-production environments!
    pub upload_witness_inputs_to_gcs: bool,

    /// Number of keys that is processed by enum_index migration in State Keeper each L1 batch.
    pub enum_index_migration_chunk_size: Option<usize>,
}

impl StateKeeperConfig {
    /// Creates a config object suitable for use in unit tests.
    /// Values mostly repeat the values used in the localhost environment.
    pub fn for_tests() -> Self {
        Self {
            transaction_slots: 250,
            block_commit_deadline_ms: 2500,
            miniblock_commit_deadline_ms: 1000,
            miniblock_seal_queue_capacity: 10,
            max_single_tx_gas: 6000000,
            max_allowed_l2_tx_gas_limit: 4000000000,
            reject_tx_at_geometry_percentage: 0.95,
            reject_tx_at_eth_params_percentage: 0.95,
            reject_tx_at_gas_percentage: 0.95,
            close_block_at_geometry_percentage: 0.95,
            close_block_at_eth_params_percentage: 0.95,
            close_block_at_gas_percentage: 0.95,
            fee_account_addr: Address::from_str("0xde03a0B5963f75f1C8485B355fF6D30f3093BDE7")
                .unwrap(),
            fair_l2_gas_price: 250000000,
            validation_computational_gas_limit: 300000,
            save_call_traces: true,
            virtual_blocks_interval: 1,
            virtual_blocks_per_miniblock: 1,
            upload_witness_inputs_to_gcs: false,
            enum_index_migration_chunk_size: None,
        }
    }

    pub fn enum_index_migration_chunk_size(&self) -> usize {
        self.enum_index_migration_chunk_size.unwrap_or(1_000)
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct OperationsManagerConfig {
    /// Sleep time in ms when there is no new input data
    pub delay_interval: u64,
}

impl OperationsManagerConfig {
    pub fn delay_interval(&self) -> Duration {
        Duration::from_millis(self.delay_interval)
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct CircuitBreakerConfig {
    pub sync_interval_ms: u64,
    pub http_req_max_retry_number: usize,
    pub http_req_retry_interval_sec: u8,
    pub replication_lag_limit_sec: Option<u32>,
}

impl CircuitBreakerConfig {
    pub fn sync_interval(&self) -> Duration {
        Duration::from_millis(self.sync_interval_ms)
    }

    pub fn http_req_retry_interval(&self) -> Duration {
        Duration::from_secs(self.http_req_retry_interval_sec as u64)
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct MempoolConfig {
    pub sync_interval_ms: u64,
    pub sync_batch_size: usize,
    pub capacity: u64,
    pub stuck_tx_timeout: u64,
    pub remove_stuck_txs: bool,
    pub delay_interval: u64,
}

impl MempoolConfig {
    pub fn sync_interval(&self) -> Duration {
        Duration::from_millis(self.sync_interval_ms)
    }

    pub fn stuck_tx_timeout(&self) -> Duration {
        Duration::from_secs(self.stuck_tx_timeout)
    }

    pub fn delay_interval(&self) -> Duration {
        Duration::from_millis(self.delay_interval)
    }
}
