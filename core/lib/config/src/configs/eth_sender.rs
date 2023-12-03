// Built-in uses
use std::time::Duration;
// External uses
use serde::Deserialize;
// Workspace uses
use zksync_basic_types::H256;

/// Configuration for the Ethereum sender crate.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ETHSenderConfig {
    /// Options related to the Ethereum sender directly.
    pub sender: SenderConfig,
    /// Options related to the `GasAdjuster` submodule.
    pub gas_adjuster: GasAdjusterConfig,
}

impl ETHSenderConfig {
    /// Creates a mock configuration object suitable for unit tests.
    /// Values inside match the config used for localhost development.
    pub fn for_tests() -> Self {
        Self {
            sender: SenderConfig {
                aggregated_proof_sizes: vec![1, 4],
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
            },
            gas_adjuster: GasAdjusterConfig {
                default_priority_fee_per_gas: 1000000000,
                max_base_fee_samples: 10000,
                pricing_formula_parameter_a: 1.5,
                pricing_formula_parameter_b: 1.0005,
                internal_l1_pricing_multiplier: 0.8,
                internal_enforced_l1_gas_price: None,
                poll_period: 5,
                max_l1_gas_price: None,
            },
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
    pub fn private_key(&self) -> Option<H256> {
        std::env::var("ETH_SENDER_SENDER_OPERATOR_PRIVATE_KEY")
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
    /// Node polling period in seconds
    pub poll_period: u64,
    /// Max number of l1 gas price that is allowed to be used in state keeper.
    pub max_l1_gas_price: Option<u64>,
}

impl GasAdjusterConfig {
    /// Converts `self.poll_period` into `Duration`.
    pub fn poll_period(&self) -> Duration {
        Duration::from_secs(self.poll_period)
    }

    pub fn max_l1_gas_price(&self) -> u64 {
        self.max_l1_gas_price.unwrap_or(u64::MAX)
    }
}
