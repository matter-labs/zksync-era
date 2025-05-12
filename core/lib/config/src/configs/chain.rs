use std::{str::FromStr, time::Duration};

use serde::{Deserialize, Serialize};
use zksync_basic_types::{commitment::L1BatchCommitmentMode, Address, H256};

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

#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
pub struct StateKeeperConfig {
    /// The max number of slots for txs in a block before it should be sealed by the slots sealer.
    pub transaction_slots: usize,

    /// Number of ms after which an L1 batch is going to be unconditionally sealed.
    pub block_commit_deadline_ms: u64,
    /// Number of ms after which an L2 block should be sealed by the timeout sealer.
    #[serde(alias = "miniblock_commit_deadline_ms")]
    // legacy naming; since we don't serialize this struct, we use "alias" rather than "rename"
    pub l2_block_commit_deadline_ms: u64,
    /// Capacity of the queue for asynchronous L2 block sealing. Once this many L2 blocks are queued,
    /// sealing will block until some of the L2 blocks from the queue are processed.
    /// 0 means that sealing is synchronous; this is mostly useful for performance comparison, testing etc.
    #[serde(alias = "miniblock_seal_queue_capacity")]
    pub l2_block_seal_queue_capacity: usize,
    /// The max payload size threshold (in bytes) that triggers sealing of an L2 block.
    #[serde(alias = "miniblock_max_payload_size")]
    pub l2_block_max_payload_size: usize,

    /// The max number of gas to spend on an L1 tx before its batch should be sealed by the gas sealer.
    pub max_single_tx_gas: u32,

    pub max_allowed_l2_tx_gas_limit: u64,

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
    /// Denotes the percentage of L1 gas used in L2 block that triggers L2 block seal.
    pub close_block_at_gas_percentage: f64,
    /// Fee account address. Value is deprecated and it's used only for generating wallets struct
    #[deprecated(note = "Use Wallets::fee_account::address instead")]
    pub fee_account_addr: Option<Address>,
    /// The minimal acceptable L2 gas price, i.e. the price that should include the cost of computation/proving as well
    /// as potentially premium for congestion.
    pub minimal_l2_gas_price: u64,
    /// The constant that represents the possibility that a batch can be sealed because of overuse of computation resources.
    /// It has range from 0 to 1. If it is 0, the compute will not depend on the cost for closing the batch.
    /// If it is 1, the gas limit per batch will have to cover the entire cost of closing the batch.
    pub compute_overhead_part: f64,
    /// The constant that represents the possibility that a batch can be sealed because of overuse of pubdata.
    /// It has range from 0 to 1. If it is 0, the pubdata will not depend on the cost for closing the batch.
    /// If it is 1, the pubdata limit per batch will have to cover the entire cost of closing the batch.
    pub pubdata_overhead_part: f64,
    /// The constant amount of L1 gas that is used as the overhead for the batch. It includes the price for batch verification, etc.
    pub batch_overhead_l1_gas: u64,
    /// The maximum amount of gas that can be used by the batch. This value is derived from the circuits limitation per batch.
    pub max_gas_per_batch: u64,
    /// The maximum amount of pubdata that can be used by the batch.
    /// This variable should not exceed:
    /// - 128kb for calldata-based rollups
    /// - 120kb * n, where `n` is a number of blobs for blob-based rollups
    /// - the DA layer's blob size limit for the DA layer-based validiums
    /// - 100 MB for the object store-based or no-da validiums
    pub max_pubdata_per_batch: u64,

    /// The version of the fee model to use.
    pub fee_model_version: FeeModelVersion,

    /// Max number of computational gas that validation step is allowed to take.
    pub validation_computational_gas_limit: u32,
    pub save_call_traces: bool,

    /// The maximal number of circuits that a batch can support.
    /// Note, that this number corresponds to the "base layer" circuits, i.e. it does not include
    /// the recursion layers' circuits.
    pub max_circuits_per_batch: usize,

    /// Configures whether to persist protective reads when persisting L1 batches in the state keeper.
    /// Protective reads can be written asynchronously in VM runner instead.
    /// By default, set to `false` as it is expected that a separate `vm_runner_protective_reads` component
    /// which is capable of saving protective reads is run.
    #[serde(default)]
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
    #[serde(default)]
    pub l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    pub deployment_allowlist: Option<DeploymentAllowlist>,
}

impl StateKeeperConfig {
    /// Creates a config object suitable for use in unit tests.
    /// Values mostly repeat the values used in the localhost environment.
    pub fn for_tests() -> Self {
        #[allow(deprecated)]
        Self {
            transaction_slots: 250,
            block_commit_deadline_ms: 2500,
            l2_block_commit_deadline_ms: 1000,
            l2_block_seal_queue_capacity: 10,
            l2_block_max_payload_size: 1_000_000,
            max_single_tx_gas: 6000000,
            max_allowed_l2_tx_gas_limit: 4000000000,
            reject_tx_at_geometry_percentage: 0.95,
            reject_tx_at_eth_params_percentage: 0.95,
            reject_tx_at_gas_percentage: 0.95,
            close_block_at_geometry_percentage: 0.95,
            close_block_at_eth_params_percentage: 0.95,
            close_block_at_gas_percentage: 0.95,
            fee_account_addr: Some(
                Address::from_str("0xde03a0B5963f75f1C8485B355fF6D30f3093BDE7").unwrap(),
            ),
            compute_overhead_part: 0.0,
            pubdata_overhead_part: 1.0,
            batch_overhead_l1_gas: 800_000,
            max_gas_per_batch: 200_000_000,
            max_pubdata_per_batch: 100_000,
            minimal_l2_gas_price: 100000000,
            fee_model_version: FeeModelVersion::V2,
            validation_computational_gas_limit: 300000,
            save_call_traces: true,
            max_circuits_per_batch: 24100,
            protective_reads_persistence_enabled: true,
            bootloader_hash: None,
            default_aa_hash: None,
            evm_emulator_hash: None,
            l1_batch_commit_data_generator_mode: L1BatchCommitmentMode::Rollup,
            deployment_allowlist: None,
        }
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

    pub fn replication_lag_limit(&self) -> Option<Duration> {
        self.replication_lag_limit_sec
            .map(|limit| Duration::from_secs(limit.into()))
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
    #[serde(default)]
    pub l1_to_l2_txs_paused: bool,
    #[serde(default)]
    pub skip_unsafe_deposit_checks: bool,
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct TimestampAsserterConfig {
    /// Minimum time between current block.timestamp and the end of the asserted range
    pub min_time_till_end_sec: u32,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub enum DeploymentAllowlist {
    Dynamic(DeploymentAllowlistDynamic),
    Static(Vec<Address>),
}

#[derive(Default, Debug, Deserialize, Clone, PartialEq)]
pub struct DeploymentAllowlistDynamic {
    /// If `Some(url)`, allowlisting is enabled. If `None`, it's disabled.
    /// If the `String` is empty, treat it as invalid and effectively disable.
    http_file_url: Option<String>,
    /// Private field for the refresh interval (in seconds).
    refresh_interval_secs: Option<u64>,
}

impl DeploymentAllowlistDynamic {
    /// Create a new `DeploymentAllowlist` instance.
    pub fn new(http_file_url: Option<String>, refresh_interval_secs: Option<u64>) -> Self {
        Self {
            http_file_url,
            refresh_interval_secs,
        }
    }

    /// Returns the allowlist file URL, if present and non-empty.
    pub fn http_file_url(&self) -> Option<&str> {
        self.http_file_url.as_deref().filter(|s| !s.is_empty())
    }

    /// Returns the refresh interval used to reload the allowlist.
    /// Defaults to 5 minutes if not set.
    pub fn refresh_interval(&self) -> Duration {
        Duration::from_secs(self.refresh_interval_secs.unwrap_or(300))
    }
}
