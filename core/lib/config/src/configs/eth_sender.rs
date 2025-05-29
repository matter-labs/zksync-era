use std::time::Duration;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use smart_config::{
    de::{Optional, Serde, WellKnown},
    metadata::TimeUnit,
    DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::{pubdata_da::PubdataSendingMode, H256};
use zksync_crypto_primitives::K256PrivateKey;

use crate::EthWatchConfig;

/// Configuration for the Ethereum related components.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct EthConfig {
    /// Options related to the Ethereum sender directly.
    #[config(nest, alias = "sender_sender")]
    pub sender: SenderConfig,
    /// Options related to the `GasAdjuster` submodule.
    #[config(nest, alias = "sender_gas_adjuster")]
    pub gas_adjuster: GasAdjusterConfig,
    #[config(nest, alias = "watch")]
    pub watcher: EthWatchConfig,
}

impl EthConfig {
    /// Creates a mock configuration object suitable for unit tests.
    /// Values inside match the config used for localhost development.
    pub fn for_tests() -> Self {
        Self {
            sender: SenderConfig {
                wait_confirmations: None,
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
                is_verifier_pre_fflonk: true,
                gas_limit_mode: GasLimitMode::Maximum,
                max_acceptable_base_fee_in_wei: 100000000000,
                time_in_mempool_multiplier_cap: None,
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
            },
            watcher: EthWatchConfig {
                confirmations_for_eth_event: None,
                eth_node_poll_interval: Duration::ZERO,
            },
        }
    }

    /// We need to modify the values inside this config. Please use this method with ultra caution,
    /// that this could be inconsistent with other codebase.
    pub fn get_eth_sender_config_for_sender_layer_data_layer(&self) -> &SenderConfig {
        &self.sender
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GasLimitMode {
    #[default]
    Maximum,
    Calculated,
}

impl WellKnown for GasLimitMode {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct SenderConfig {
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
    pub proof_sending_mode: ProofSendingMode,
    #[config(default_t = 4_000_000)]
    pub max_aggregated_tx_gas: u64,
    #[config(default_t = 6_000_000)]
    pub max_eth_tx_data_size: usize,
    #[config(default_t = 10)]
    pub max_aggregated_blocks_to_commit: u32,
    #[config(default_t = 10)]
    pub max_aggregated_blocks_to_execute: u32,
    #[config(default_t = 5 * TimeUnit::Minutes, with = TimeUnit::Seconds)]
    pub aggregated_block_commit_deadline: Duration,
    #[config(default_t = 5 * TimeUnit::Minutes, with = TimeUnit::Seconds)]
    pub aggregated_block_prove_deadline: Duration,
    #[config(default_t = 5 * TimeUnit::Minutes, with = TimeUnit::Seconds)]
    pub aggregated_block_execute_deadline: Duration,
    #[config(default_t = 30)]
    pub timestamp_criteria_max_allowed_lag: usize,

    /// L1 batches will only be executed on L1 contract after they are at least this number of seconds old.
    /// Note that this number must be slightly higher than the one set on the contract,
    /// because the contract uses `block.timestamp` which lags behind the clock time.
    #[config(with = Optional(TimeUnit::Seconds))]
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
    #[config(default_t = true)]
    pub is_verifier_pre_fflonk: bool,
    #[config(default)]
    pub gas_limit_mode: GasLimitMode,
    /// Max acceptable base fee the sender is allowed to use to send L1 txs.
    #[config(default_t = u64::MAX)]
    pub max_acceptable_base_fee_in_wei: u64,
    /// Cap for `b ^ time_in_mempool` used for price calculations.
    #[config(default)]
    pub time_in_mempool_multiplier_cap: Option<u32>,
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

    const fn default_time_in_mempool_in_l1_blocks_cap() -> u32 {
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
    // TODO(EVM-920): Note, that while the name says "L1", this same parameter is actually used for
    //   any settlement layer.
    #[config(default_t = 1.0)]
    pub internal_l1_pricing_multiplier: f64,
    /// If equal to Some(x), then it will always provide `x` as the L1 gas price
    // TODO(EVM-920): Note, that while the name says "L1", this same parameter is actually used for
    //   any settlement layer.
    #[config(default)]
    pub internal_enforced_l1_gas_price: Option<u64>,
    /// If equal to Some(x), then it will always provide `x` as the pubdata price
    #[config(default)]
    pub internal_enforced_pubdata_price: Option<u64>,
    /// Node polling period in seconds
    #[config(default_t = 1 * TimeUnit::Minutes, with = TimeUnit::Seconds)]
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
}

#[cfg(test)]
mod tests {
    use smart_config::{
        testing::{test, Tester},
        Environment, Yaml,
    };

    use super::*;

    fn expected_config() -> EthConfig {
        EthConfig {
            sender: SenderConfig {
                aggregated_block_commit_deadline: Duration::from_secs(30),
                aggregated_block_prove_deadline: Duration::from_secs(3_000),
                aggregated_block_execute_deadline: Duration::from_secs(4_000),
                max_aggregated_tx_gas: 4_000_000,
                max_eth_tx_data_size: 120_000,
                timestamp_criteria_max_allowed_lag: 30,
                max_aggregated_blocks_to_commit: 3,
                max_aggregated_blocks_to_execute: 4,
                wait_confirmations: Some(1),
                tx_poll_period: Duration::from_secs(3),
                aggregate_tx_poll_period: Duration::from_secs(3),
                max_txs_in_flight: 3,
                proof_sending_mode: ProofSendingMode::SkipEveryProof,
                l1_batch_min_age_before_execute_seconds: Some(Duration::from_secs(1000)),
                max_acceptable_priority_fee_in_gwei: 100_000_000_000,
                pubdata_sending_mode: PubdataSendingMode::Calldata,
                tx_aggregation_only_prove_and_execute: false,
                tx_aggregation_paused: false,
                time_in_mempool_in_l1_blocks_cap: 2000,
                is_verifier_pre_fflonk: false,
                gas_limit_mode: GasLimitMode::Calculated,
                max_acceptable_base_fee_in_wei: 100_000_000_000,
                time_in_mempool_multiplier_cap: Some(10),
            },
            gas_adjuster: GasAdjusterConfig {
                default_priority_fee_per_gas: 20000000000,
                max_base_fee_samples: 10000,
                pricing_formula_parameter_a: 1.5,
                pricing_formula_parameter_b: 1.0005,
                internal_l1_pricing_multiplier: 0.8,
                internal_enforced_l1_gas_price: Some(10000000),
                internal_enforced_pubdata_price: Some(5000000),
                poll_period: Duration::from_secs(15),
                max_l1_gas_price: 100000000,
                num_samples_for_blob_base_fee_estimate: 10,
                internal_pubdata_pricing_multiplier: 1.0,
                max_blob_base_fee: 1000,
            },
            watcher: EthWatchConfig {
                confirmations_for_eth_event: Some(0),
                eth_node_poll_interval: Duration::from_millis(300),
            },
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            ETH_WATCH_CONFIRMATIONS_FOR_ETH_EVENT="0"
            ETH_WATCH_ETH_NODE_POLL_INTERVAL="300"
            ETH_SENDER_SENDER_WAIT_CONFIRMATIONS="1"
            ETH_SENDER_SENDER_TX_POLL_PERIOD="3"
            ETH_SENDER_SENDER_AGGREGATE_TX_POLL_PERIOD="3"
            ETH_SENDER_SENDER_MAX_TXS_IN_FLIGHT="3"
            ETH_SENDER_SENDER_OPERATOR_PRIVATE_KEY="0x27593fea79697e947890ecbecce7901b0008345e5d7259710d0dd5e500d040be"
            ETH_SENDER_SENDER_PROOF_SENDING_MODE="SkipEveryProof"
            ETH_SENDER_GAS_ADJUSTER_DEFAULT_PRIORITY_FEE_PER_GAS="20000000000"
            ETH_SENDER_GAS_ADJUSTER_MAX_BASE_FEE_SAMPLES="10000"
            ETH_SENDER_GAS_ADJUSTER_PRICING_FORMULA_PARAMETER_A="1.5"
            ETH_SENDER_GAS_ADJUSTER_PRICING_FORMULA_PARAMETER_B="1.0005"
            ETH_SENDER_GAS_ADJUSTER_INTERNAL_L1_PRICING_MULTIPLIER="0.8"
            ETH_SENDER_GAS_ADJUSTER_POLL_PERIOD="15"
            ETH_SENDER_GAS_ADJUSTER_MAX_L1_GAS_PRICE="100000000"
            ETH_SENDER_GAS_ADJUSTER_MAX_BLOB_BASE_FEE=1000
            ETH_SENDER_GAS_ADJUSTER_INTERNAL_PUBDATA_PRICING_MULTIPLIER="1.0"
            ETH_SENDER_GAS_ADJUSTER_INTERNAL_ENFORCED_L1_GAS_PRICE=10000000
            ETH_SENDER_GAS_ADJUSTER_INTERNAL_ENFORCED_PUBDATA_PRICE=5000000
            ETH_SENDER_SENDER_AGGREGATED_PROOF_SIZES="1,5"
            ETH_SENDER_SENDER_MAX_AGGREGATED_BLOCKS_TO_COMMIT="3"
            ETH_SENDER_SENDER_MAX_AGGREGATED_BLOCKS_TO_EXECUTE="4"
            ETH_SENDER_SENDER_AGGREGATED_BLOCK_COMMIT_DEADLINE="30"
            ETH_SENDER_SENDER_AGGREGATED_BLOCK_PROVE_DEADLINE="3000"
            ETH_SENDER_SENDER_AGGREGATED_BLOCK_EXECUTE_DEADLINE="4000"
            ETH_SENDER_SENDER_TIMESTAMP_CRITERIA_MAX_ALLOWED_LAG="30"
            ETH_SENDER_SENDER_MAX_AGGREGATED_TX_GAS="4000000"
            ETH_SENDER_SENDER_MAX_ETH_TX_DATA_SIZE="120000"
            ETH_SENDER_SENDER_TIME_IN_MEMPOOL_IN_L1_BLOCKS_CAP="2000"
            ETH_SENDER_SENDER_L1_BATCH_MIN_AGE_BEFORE_EXECUTE_SECONDS="1000"
            ETH_SENDER_SENDER_MAX_ACCEPTABLE_PRIORITY_FEE_IN_GWEI="100000000000"
            ETH_SENDER_SENDER_PUBDATA_SENDING_MODE="Calldata"
            ETH_SENDER_SENDER_IS_VERIFIER_PRE_FFLONK=false
            ETH_SENDER_SENDER_GAS_LIMIT_MODE=Calculated
            ETH_SENDER_SENDER_MAX_ACCEPTABLE_BASE_FEE_IN_WEI=100000000000
            ETH_SENDER_SENDER_TIME_IN_MEMPOOL_MULTIPLIER_CAP="10"
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("ETH_");

        let config: EthConfig = test(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          sender:
            wait_confirmations: 1
            tx_poll_period: 3
            aggregate_tx_poll_period: 3
            l1_batch_min_age_before_execute_seconds: 1000
            max_txs_in_flight: 3
            proof_sending_mode: SKIP_EVERY_PROOF
            max_aggregated_tx_gas: 4000000
            max_eth_tx_data_size: 120000
            max_aggregated_blocks_to_commit: 3
            max_aggregated_blocks_to_execute: 4
            aggregated_block_commit_deadline: 30
            aggregated_block_prove_deadline: 3000
            aggregated_block_execute_deadline: 4000
            timestamp_criteria_max_allowed_lag: 30
            max_acceptable_priority_fee_in_gwei: 100000000000
            pubdata_sending_mode: CALLDATA
            tx_aggregation_paused: false
            tx_aggregation_only_prove_and_execute: false
            time_in_mempool_in_l1_blocks_cap: 2000
            is_verifier_pre_fflonk: false
            gas_limit_mode: Calculated
            max_acceptable_base_fee_in_wei: 100000000000
            time_in_mempool_multiplier_cap: 10
          gas_adjuster:
            default_priority_fee_per_gas: 20000000000
            max_base_fee_samples: 10000
            max_l1_gas_price: 100000000
            pricing_formula_parameter_a: 1.5
            pricing_formula_parameter_b: 1.0005
            internal_l1_pricing_multiplier: 0.8
            poll_period: 15
            num_samples_for_blob_base_fee_estimate: 10
            settlement_mode: "SettlesToL1"
            internal_pubdata_pricing_multiplier: 1.0
            internal_enforced_l1_gas_price: 10000000
            internal_enforced_pubdata_price: 5000000
            max_blob_base_fee: 1000
          watcher:
            confirmations_for_eth_event: 0
            eth_node_poll_interval: 300
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: EthConfig = Tester::default()
            .coerce_variant_names()
            .test_complete(yaml)
            .unwrap();
        assert_eq!(config, expected_config());
    }
}
