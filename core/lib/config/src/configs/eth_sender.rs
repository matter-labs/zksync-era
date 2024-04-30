use std::time::Duration;

use anyhow::Context as _;
use serde::Deserialize;
use zksync_basic_types::H256;
use zksync_crypto_primitives::K256PrivateKey;

use crate::EthWatchConfig;

/// Configuration for the Ethereum related components.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct EthConfig {
    /// Options related to the Ethereum sender directly.
    pub sender: Option<SenderConfig>,
    /// Options related to the `GasAdjuster` submodule.
    pub gas_adjuster: Option<GasAdjusterConfig>,
    pub watcher: Option<EthWatchConfig>,
    pub web3_url: String,
}

impl EthConfig {
    /// Creates a mock configuration object suitable for unit tests.
    /// Values inside match the config used for localhost development.
    pub fn for_tests() -> Self {
        Self {
            sender: Some(SenderConfig {
                aggregated_proof_sizes: vec![1],
                wait_confirmations: Some(1),
                tx_poll_period: 1,
                aggregate_tx_poll_period: 1,
                max_txs_in_flight: 30,
                proof_sending_mode: ProofSendingMode::SkipEveryProof,
                max_aggregated_tx_gas: 4000000,
                max_eth_tx_data_size: 6000000,
                max_aggregated_blocks_to_commit: 10,
                max_aggregated_blocks_to_execute: 10,
                aggregated_block_commit_deadline: 1,
                aggregated_block_prove_deadline: 10,
                aggregated_block_execute_deadline: 10,
                timestamp_criteria_max_allowed_lag: 30,
                l1_batch_min_age_before_execute_seconds: None,
                max_acceptable_priority_fee_in_gwei: 100000000000,
                proof_loading_mode: ProofLoadingMode::OldProofFromDb,
                pubdata_sending_mode: PubdataSendingMode::Calldata,
            }),
            gas_adjuster: Some(GasAdjusterConfig {
                default_priority_fee_per_gas: 1000000000,
                max_base_fee_samples: 10000,
                pricing_formula_parameter_a: 1.5,
                pricing_formula_parameter_b: 1.0005,
                internal_l1_pricing_multiplier: 0.8,
                internal_enforced_l1_gas_price: None,
                internal_enforced_pubdata_price: None,
                poll_period: 5,
                max_l1_gas_price: None,
                num_samples_for_blob_base_fee_estimate: 10,
                internal_pubdata_pricing_multiplier: 1.0,
                max_blob_base_fee: None,
            }),
            watcher: Some(EthWatchConfig {
                confirmations_for_eth_event: None,
                eth_node_poll_interval: 0,
            }),
            web3_url: "localhost:8545".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub enum ProofSendingMode {
    OnlyRealProofs,
    OnlySampledProofs,
    SkipEveryProof,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub enum ProofLoadingMode {
    OldProofFromDb,
    FriProofFromGcs,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Default)]
pub enum PubdataSendingMode {
    #[default]
    Calldata,
    Blobs,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct SenderConfig {
    pub aggregated_proof_sizes: Vec<usize>,
    /// Amount of confirmations required to consider L1 transaction committed.
    /// If not specified L1 transaction will be considered finalized once its block is finalized.
    pub wait_confirmations: Option<u64>,
    /// Node polling period in seconds.
    pub tx_poll_period: u64,
    /// Aggregate txs polling period in seconds.
    pub aggregate_tx_poll_period: u64,
    /// The maximum number of unconfirmed Ethereum transactions.
    pub max_txs_in_flight: u64,
    /// The mode in which proofs are sent.
    pub proof_sending_mode: ProofSendingMode,

    pub max_aggregated_tx_gas: u32,
    pub max_eth_tx_data_size: usize,
    pub max_aggregated_blocks_to_commit: u32,
    pub max_aggregated_blocks_to_execute: u32,
    pub aggregated_block_commit_deadline: u64,
    pub aggregated_block_prove_deadline: u64,
    pub aggregated_block_execute_deadline: u64,
    pub timestamp_criteria_max_allowed_lag: usize,

    /// L1 batches will only be executed on L1 contract after they are at least this number of seconds old.
    /// Note that this number must be slightly higher than the one set on the contract,
    /// because the contract uses block.timestamp which lags behind the clock time.
    pub l1_batch_min_age_before_execute_seconds: Option<u64>,
    // Max acceptable fee for sending tx it acts as a safeguard to prevent sending tx with very high fees.
    pub max_acceptable_priority_fee_in_gwei: u64,

    /// The mode in which proofs are loaded, either from DB/GCS for FRI/Old proof.
    pub proof_loading_mode: ProofLoadingMode,

    /// The mode in which we send pubdata, either Calldata or Blobs
    pub pubdata_sending_mode: PubdataSendingMode,
}

impl SenderConfig {
    /// Converts `self.tx_poll_period` into `Duration`.
    pub fn tx_poll_period(&self) -> Duration {
        Duration::from_secs(self.tx_poll_period)
    }

    /// Converts `self.aggregate_tx_poll_period` into `Duration`.
    pub fn aggregate_tx_poll_period(&self) -> Duration {
        Duration::from_secs(self.aggregate_tx_poll_period)
    }

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
}

#[derive(Debug, Deserialize, Copy, Clone, PartialEq)]
pub struct GasAdjusterConfig {
    /// Priority Fee to be used by GasAdjuster
    pub default_priority_fee_per_gas: u64,
    /// Number of blocks collected by GasAdjuster from which base_fee median is taken
    pub max_base_fee_samples: usize,
    /// Parameter of the transaction base_fee_per_gas pricing formula
    pub pricing_formula_parameter_a: f64,
    /// Parameter of the transaction base_fee_per_gas pricing formula
    pub pricing_formula_parameter_b: f64,
    /// Parameter by which the base fee will be multiplied for internal purposes
    pub internal_l1_pricing_multiplier: f64,
    /// If equal to Some(x), then it will always provide `x` as the L1 gas price
    pub internal_enforced_l1_gas_price: Option<u64>,
    /// If equal to Some(x), then it will always provide `x` as the pubdata price
    pub internal_enforced_pubdata_price: Option<u64>,
    /// Node polling period in seconds
    pub poll_period: u64,
    /// Max number of l1 gas price that is allowed to be used.
    pub max_l1_gas_price: Option<u64>,
    /// Number of blocks collected by GasAdjuster from which `blob_base_fee` median is taken
    #[serde(default = "GasAdjusterConfig::default_num_samples_for_blob_base_fee_estimate")]
    pub num_samples_for_blob_base_fee_estimate: usize,
    /// Parameter by which the pubdata fee will be multiplied for internal purposes
    #[serde(default = "GasAdjusterConfig::default_internal_pubdata_pricing_multiplier")]
    pub internal_pubdata_pricing_multiplier: f64,
    /// Max blob base fee that is allowed to be used.
    pub max_blob_base_fee: Option<u64>,
}

impl GasAdjusterConfig {
    /// Converts `self.poll_period` into `Duration`.
    pub fn poll_period(&self) -> Duration {
        Duration::from_secs(self.poll_period)
    }

    pub fn max_l1_gas_price(&self) -> u64 {
        self.max_l1_gas_price.unwrap_or(u64::MAX)
    }

    pub fn max_blob_base_fee(&self) -> u64 {
        self.max_blob_base_fee.unwrap_or(u64::MAX)
    }

    pub const fn default_num_samples_for_blob_base_fee_estimate() -> usize {
        10
    }

    pub const fn default_internal_pubdata_pricing_multiplier() -> f64 {
        1.0
    }
}
