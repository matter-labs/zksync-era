use std::time::Duration;

use anyhow::Context as _;
use serde::Deserialize;
use smart_config::{
    de::{Optional, Serde, WellKnown},
    metadata::TimeUnit,
    DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::{pubdata_da::PubdataSendingMode, settlement::SettlementMode, H256};
use zksync_crypto_primitives::K256PrivateKey;

use crate::EthWatchConfig;

/// Configuration for the Ethereum related components.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct EthConfig {
    /// Options related to the Ethereum sender directly.
    #[config(nest)]
    pub sender: SenderConfig,
    /// Options related to the `GasAdjuster` submodule.
    #[config(nest, default)]
    pub gas_adjuster: GasAdjusterConfig,
    #[config(nest, default)]
    pub watcher: EthWatchConfig,
}

impl EthConfig {
    /// Creates a mock configuration object suitable for unit tests.
    /// Values inside match the config used for localhost development.
    pub fn for_tests() -> Self {
        Self {
            sender: SenderConfig {
                aggregated_proof_sizes: vec![1],
                wait_confirmations: Some(10),
                tx_poll_period: Duration::from_secs(1),
                aggregate_tx_poll_period: Duration::from_secs(1),
                max_txs_in_flight: 30,
                proof_sending_mode: ProofSendingMode::SkipEveryProof,
                max_aggregated_tx_gas: 4000000,
                max_eth_tx_data_size: 6000000,
                max_aggregated_blocks_to_commit: 10,
                max_aggregated_blocks_to_execute: 10,
                aggregated_block_commit_deadline: Duration::from_secs(1),
                aggregated_block_prove_deadline: Duration::from_secs(10),
                aggregated_block_execute_deadline: Duration::from_secs(10),
                timestamp_criteria_max_allowed_lag: 30,
                l1_batch_min_age_before_execute_seconds: None,
                max_acceptable_priority_fee_in_gwei: 100000000000,
                pubdata_sending_mode: PubdataSendingMode::Calldata,
                tx_aggregation_paused: false,
                tx_aggregation_only_prove_and_execute: false,
                time_in_mempool_in_l1_blocks_cap: 1800,
            },
            gas_adjuster: GasAdjusterConfig {
                default_priority_fee_per_gas: 1000000000,
                max_base_fee_samples: 10000,
                pricing_formula_parameter_a: 1.5,
                pricing_formula_parameter_b: 1.0005,
                internal_l1_pricing_multiplier: 0.8,
                internal_enforced_l1_gas_price: None,
                internal_enforced_pubdata_price: None,
                poll_period: Duration::from_secs(5),
                max_l1_gas_price: u64::MAX,
                num_samples_for_blob_base_fee_estimate: 10,
                internal_pubdata_pricing_multiplier: 1.0,
                max_blob_base_fee: u64::MAX,
                settlement_mode: Default::default(),
            },
            watcher: EthWatchConfig {
                confirmations_for_eth_event: None,
                eth_node_poll_interval: Duration::ZERO,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub enum ProofSendingMode {
    OnlyRealProofs,
    OnlySampledProofs,
    SkipEveryProof,
}

impl WellKnown for ProofSendingMode {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProofLoadingMode {
    OldProofFromDb,
    FriProofFromGcs,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct SenderConfig {
    #[config(default_t = vec![1])]
    pub aggregated_proof_sizes: Vec<usize>,
    /// Amount of confirmations required to consider L1 transaction committed.
    /// If not specified L1 transaction will be considered finalized once its block is finalized.
    pub wait_confirmations: Option<u64>,
    /// Node polling period in seconds.
    #[config(default_t = Duration::from_secs(1), with = TimeUnit::Seconds)]
    pub tx_poll_period: Duration,
    /// Aggregate txs polling period in seconds.
    #[config(default_t = Duration::from_secs(1), with = TimeUnit::Seconds)]
    pub aggregate_tx_poll_period: Duration,
    /// The maximum number of unconfirmed Ethereum transactions.
    #[config(default_t = 30)]
    pub max_txs_in_flight: u64,
    /// The mode in which proofs are sent.
    #[config(default_t = ProofSendingMode::SkipEveryProof)]
    pub proof_sending_mode: ProofSendingMode,
    #[config(default_t = 4_000_000)]
    pub max_aggregated_tx_gas: u32,
    #[config(default_t = 6_000_000)]
    pub max_eth_tx_data_size: usize,
    #[config(default_t = 10)]
    pub max_aggregated_blocks_to_commit: u32,
    #[config(default_t = 10)]
    pub max_aggregated_blocks_to_execute: u32,
    #[config(default_t = Duration::from_secs(300), with = TimeUnit::Seconds)]
    pub aggregated_block_commit_deadline: Duration,
    #[config(default_t = Duration::from_secs(300), with = TimeUnit::Seconds)]
    pub aggregated_block_prove_deadline: Duration,
    #[config(default_t = Duration::from_secs(300), with = TimeUnit::Seconds)]
    pub aggregated_block_execute_deadline: Duration,
    // FIXME: existing configs assume that this is measured in seconds, but it's not; it's measured in L1 batches
    #[config(default_t = 30)]
    pub timestamp_criteria_max_allowed_lag: usize,

    /// L1 batches will only be executed on L1 contract after they are at least this number of seconds old.
    /// Note that this number must be slightly higher than the one set on the contract,
    /// because the contract uses `block.timestamp` which lags behind the clock time.
    #[config(default, with = Optional(TimeUnit::Seconds))]
    pub l1_batch_min_age_before_execute_seconds: Option<Duration>,
    // Max acceptable fee for sending tx it acts as a safeguard to prevent sending tx with very high fees.
    #[config(default_t = 100_000_000_000)]
    pub max_acceptable_priority_fee_in_gwei: u64,

    /// The mode in which we send pubdata: Calldata, Blobs or Custom (DA layers, Object Store, etc.)
    #[config(with = Serde![str])]
    pub pubdata_sending_mode: PubdataSendingMode,
    /// Special mode specifically for gateway migration to allow all inflight txs to be processed.
    #[config(default)]
    pub tx_aggregation_paused: bool,
    /// Special mode specifically for gateway migration to decrease number of non-executed batches.
    #[config(default)]
    pub tx_aggregation_only_prove_and_execute: bool,
    /// Cap of time in mempool for price calculations
    #[config(default = SenderConfig::default_time_in_mempool_in_l1_blocks_cap)]
    pub time_in_mempool_in_l1_blocks_cap: u32,
}

impl SenderConfig {
    // Don't load private key, if it's not required.
    #[deprecated]
    pub fn private_key(&self) -> anyhow::Result<Option<K256PrivateKey>> {
        std::env::var("ETH_SENDER_SENDER_OPERATOR_PRIVATE_KEY")
            .ok()
            .map(|pk| {
                let private_key_bytes: H256 =
                    pk.parse().context("failed parsing private key bytes")?;
                K256PrivateKey::from_bytes(private_key_bytes)
                    .context("private key bytes are invalid")
            })
            .transpose()
    }

    // Don't load blobs private key, if it's not required
    #[deprecated]
    pub fn private_key_blobs(&self) -> Option<H256> {
        std::env::var("ETH_SENDER_SENDER_OPERATOR_BLOBS_PRIVATE_KEY")
            .ok()
            .map(|pk| pk.parse().unwrap())
    }

    pub const fn default_time_in_mempool_in_l1_blocks_cap() -> u32 {
        let blocks_per_hour = 3600 / 12;
        // we cap it at 6h to not allow nearly infinite values when a tx is stuck for a long time
        // 1,001 ^ 1800 ~= 6, so by default we cap exponential price formula at roughly median * 6
        blocks_per_hour * 6
    }
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct GasAdjusterConfig {
    /// Priority Fee to be used by GasAdjuster
    #[config(default_t = 1_000_000_000)]
    pub default_priority_fee_per_gas: u64,
    /// Number of blocks collected by GasAdjuster from which base_fee median is taken
    #[config(default_t = 100)]
    pub max_base_fee_samples: usize,
    /// Parameter of the transaction base_fee_per_gas pricing formula
    #[config(default_t = 1.1)]
    pub pricing_formula_parameter_a: f64,
    /// Parameter of the transaction base_fee_per_gas pricing formula
    #[config(default_t = 1.001)]
    pub pricing_formula_parameter_b: f64,
    /// Parameter by which the base fee will be multiplied for internal purposes
    #[config(default_t = 1.0)]
    pub internal_l1_pricing_multiplier: f64,
    /// If equal to Some(x), then it will always provide `x` as the L1 gas price
    #[config(default)]
    pub internal_enforced_l1_gas_price: Option<u64>,
    /// If equal to Some(x), then it will always provide `x` as the pubdata price
    #[config(default)]
    pub internal_enforced_pubdata_price: Option<u64>,
    /// Node polling period in seconds
    #[config(default_t = Duration::from_secs(60), with = TimeUnit::Seconds)]
    pub poll_period: Duration,
    /// Max number of l1 gas price that is allowed to be used.
    #[config(default_t = u64::MAX)]
    pub max_l1_gas_price: u64,
    /// Number of blocks collected by GasAdjuster from which `blob_base_fee` median is taken
    #[config(default_t = 10)]
    pub num_samples_for_blob_base_fee_estimate: usize,
    /// Parameter by which the pubdata fee will be multiplied for internal purposes
    #[config(default_t = 1.0)]
    pub internal_pubdata_pricing_multiplier: f64,
    /// Max blob base fee that is allowed to be used.
    #[config(default_t = u64::MAX)]
    pub max_blob_base_fee: u64,
    /// Whether the gas adjuster should require that the L2 node is used as a settlement layer.
    /// It offers a runtime check for correctly provided values.
    #[config(default, with = Serde![str])]
    pub settlement_mode: SettlementMode,
}
