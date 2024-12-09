#![allow(deprecated)] // FIXME

use std::{str::FromStr, time::Duration};

use serde::Deserialize;
use smart_config::{
    de::{Optional, Serde},
    metadata::{SizeUnit, TimeUnit},
    ByteSize, DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::{
    commitment::L1BatchCommitmentMode, network::Network, Address, L2ChainId, H256,
};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct NetworkConfig {
    /// Name of the used Ethereum network, e.g. `localhost` or `rinkeby`.
    #[config(with = Serde![str], default_t = Network::Localhost)]
    pub network: Network,
    /// Name of current ZKsync network
    /// Used for Sentry environment
    #[config(default_t = "localhost".into())]
    pub zksync_network: String,
    /// ID of current ZKsync network treated as ETH network ID.
    /// Used to distinguish ZKsync from other Web3-capable networks.
    #[config(default, with = Serde![int])]
    pub zksync_network_id: L2ChainId,
}

/// An enum that represents the version of the fee model to use.
///  - `V1`, the first model that was used in ZKsync Era. In this fee model, the pubdata price must be pegged to the L1 gas price.
///    Also, the fair L2 gas price is expected to only include the proving/computation price for the operator and not the costs that come from
///    processing the batch on L1.
///  - `V2`, the second model that was used in ZKsync Era. There the pubdata price might be independent from the L1 gas price. Also,
///    The fair L2 gas price is expected to both the proving/computation price for the operator and the costs that come from
///    processing the batch on L1.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum FeeModelVersion {
    V1,
    V2,
}

impl Default for FeeModelVersion {
    fn default() -> Self {
        Self::V1
    }
}

// FIXME: which of these params have reasonable defaults?
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct StateKeeperConfig {
    /// The max number of slots for txs in a block before it should be sealed by the slots sealer.
    #[config(default_t = 8_192)]
    pub transaction_slots: usize,

    /// Number of ms after which an L1 batch is going to be unconditionally sealed.
    #[config(alias = "block_commit_deadline_ms")]
    #[config(default_t = Duration::from_millis(2_500), with = TimeUnit::Millis)]
    pub l1_batch_commit_deadline_ms: Duration,
    /// Number of ms after which an L2 block should be sealed by the timeout sealer.
    #[config(alias = "miniblock_commit_deadline_ms")]
    #[config(default_t = Duration::from_secs(1), with = TimeUnit::Millis)]
    // ^ legacy naming; since we don't serialize this struct, we use "alias" rather than "rename"
    pub l2_block_commit_deadline_ms: Duration,
    /// Capacity of the queue for asynchronous L2 block sealing. Once this many L2 blocks are queued,
    /// sealing will block until some of the L2 blocks from the queue are processed.
    /// 0 means that sealing is synchronous; this is mostly useful for performance comparison, testing etc.
    #[config(alias = "miniblock_seal_queue_capacity")]
    #[config(default_t = 10)]
    pub l2_block_seal_queue_capacity: usize,
    /// The max payload size threshold (in bytes) that triggers sealing of an L2 block.
    #[config(alias = "miniblock_max_payload_size")]
    #[config(default_t = ByteSize(1_000_000), with = SizeUnit::Bytes)]
    pub l2_block_max_payload_size: ByteSize,

    /// The max number of gas to spend on an L1 tx before its batch should be sealed by the gas sealer.
    #[config(default_t = 15_000_000)]
    pub max_single_tx_gas: u32,
    #[config(default_t = 15_000_000_000)]
    pub max_allowed_l2_tx_gas_limit: u64,

    /// Configuration option for tx to be rejected in case
    /// it takes more percentage of the block capacity than this value.
    #[config(default_t = 0.95)]
    pub reject_tx_at_geometry_percentage: f64,
    /// Configuration option for tx to be rejected in case
    /// it takes more percentage of the block capacity than this value.
    #[config(default_t = 0.95)]
    pub reject_tx_at_eth_params_percentage: f64,
    /// Configuration option for tx to be rejected in case
    /// it takes more percentage of the block capacity than this value.
    #[config(default_t = 0.95)]
    pub reject_tx_at_gas_percentage: f64,
    /// Denotes the percentage of geometry params used in L2 block that triggers L2 block seal.
    #[config(default_t = 0.95)]
    pub close_block_at_geometry_percentage: f64,
    /// Denotes the percentage of L1 params used in L2 block that triggers L2 block seal.
    #[config(default_t = 0.95)]
    pub close_block_at_eth_params_percentage: f64,
    /// Denotes the percentage of L1 gas used in L2 block that triggers L2 block seal.
    #[config(default_t = 0.95)]
    pub close_block_at_gas_percentage: f64,
    /// Fee account address. Value is deprecated and it's used only for generating wallets struct
    #[deprecated(note = "Use Wallets::fee_account::address instead")]
    pub fee_account_addr: Option<Address>,
    /// The minimal acceptable L2 gas price, i.e. the price that should include the cost of computation/proving as well
    /// as potentially premium for congestion.
    #[config(default_t = 100_000_000)]
    pub minimal_l2_gas_price: u64,
    /// The constant that represents the possibility that a batch can be sealed because of overuse of computation resources.
    /// It has range from 0 to 1. If it is 0, the compute will not depend on the cost for closing the batch.
    /// If it is 1, the gas limit per batch will have to cover the entire cost of closing the batch.
    #[config(default_t = 0.0)]
    pub compute_overhead_part: f64,
    /// The constant that represents the possibility that a batch can be sealed because of overuse of pubdata.
    /// It has range from 0 to 1. If it is 0, the pubdata will not depend on the cost for closing the batch.
    /// If it is 1, the pubdata limit per batch will have to cover the entire cost of closing the batch.
    #[config(default_t = 1.0)]
    pub pubdata_overhead_part: f64,
    /// The constant amount of L1 gas that is used as the overhead for the batch. It includes the price for batch verification, etc.
    #[config(default_t = 800_000)]
    pub batch_overhead_l1_gas: u64,
    /// The maximum amount of gas that can be used by the batch. This value is derived from the circuits limitation per batch.
    #[config(default_t = 200_000_000)]
    pub max_gas_per_batch: u64,
    /// The maximum amount of pubdata that can be used by the batch.
    /// This variable should not exceed:
    /// - 128kb for calldata-based rollups
    /// - 120kb * n, where `n` is a number of blobs for blob-based rollups
    /// - the DA layer's blob size limit for the DA layer-based validiums
    /// - 100 MB for the object store-based or no-da validiums
    #[config(default_t = ByteSize(500_000), with = SizeUnit::Bytes)]
    pub max_pubdata_per_batch: ByteSize,

    /// The version of the fee model to use.
    #[config(default_t = FeeModelVersion::V2, with = Serde![str])]
    pub fee_model_version: FeeModelVersion,

    /// Max number of computational gas that validation step is allowed to take.
    #[config(default_t = 300_000)]
    pub validation_computational_gas_limit: u32,
    #[config(default_t = true)]
    pub save_call_traces: bool,

    /// The maximal number of circuits that a batch can support.
    /// Note, that this number corresponds to the "base layer" circuits, i.e. it does not include
    /// the recursion layers' circuits.
    #[config(default_t = 31_100)]
    pub max_circuits_per_batch: usize,

    /// Configures whether to persist protective reads when persisting L1 batches in the state keeper.
    /// Protective reads can be written asynchronously in VM runner instead.
    /// By default, set to `false` as it is expected that a separate `vm_runner_protective_reads` component
    /// which is capable of saving protective reads is run.
    #[config(default)]
    pub protective_reads_persistence_enabled: bool,

    // Base system contract hashes, required only for generating genesis config.
    // #PLA-811
    #[deprecated(note = "Use GenesisConfig::bootloader_hash instead")]
    pub bootloader_hash: Option<H256>,
    #[deprecated(note = "Use GenesisConfig::default_aa_hash instead")]
    pub default_aa_hash: Option<H256>,
    #[deprecated(note = "Use GenesisConfig::evm_emulator_hash instead")]
    pub evm_emulator_hash: Option<H256>,
    #[deprecated(note = "Use GenesisConfig::l1_batch_commit_data_generator_mode instead")]
    #[config(default, with = Serde![str])]
    pub l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
}

impl StateKeeperConfig {
    /// Creates a config object suitable for use in unit tests.
    /// Values mostly repeat the values used in the localhost environment.
    pub fn for_tests() -> Self {
        #[allow(deprecated)]
        Self {
            fee_account_addr: Some(
                Address::from_str("0xde03a0B5963f75f1C8485B355fF6D30f3093BDE7").unwrap(),
            ),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct OperationsManagerConfig {
    /// Sleep time in ms when there is no new input data
    #[config(default_t = Duration::from_millis(100), with = TimeUnit::Millis)]
    pub delay_interval: Duration,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct CircuitBreakerConfig {
    #[config(default_t = Duration::from_secs(120), with = TimeUnit::Millis)]
    pub sync_interval_ms: Duration,
    #[config(default_t = 10)]
    pub http_req_max_retry_number: usize,
    #[config(default_t = Duration::from_secs(2), with = TimeUnit::Seconds)]
    pub http_req_retry_interval_sec: Duration,
    #[config(default_t = Some(Duration::from_secs(100)), with = Optional(TimeUnit::Seconds))]
    pub replication_lag_limit_sec: Option<Duration>,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct MempoolConfig {
    #[config(default_t = Duration::from_millis(10), with = TimeUnit::Millis)]
    pub sync_interval_ms: Duration,
    #[config(default_t = 1_000)]
    pub sync_batch_size: usize,
    #[config(default_t = 10_000_000)]
    pub capacity: u64,
    #[config(default_t = Duration::from_secs(172_800), with = TimeUnit::Seconds)]
    pub stuck_tx_timeout: Duration,
    #[config(default_t = true)]
    pub remove_stuck_txs: bool,
    #[config(default_t = Duration::from_millis(100), with = TimeUnit::Millis)]
    pub delay_interval: Duration,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct TimestampAsserterConfig {
    /// Minimum time between current block.timestamp and the end of the asserted range
    #[config(default_t = Duration::from_secs(60), with = TimeUnit::Seconds)]
    pub min_time_till_end_sec: Duration,
}
