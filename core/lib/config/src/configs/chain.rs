use std::{collections::HashSet, time::Duration};

use serde::{Deserialize, Serialize};
use smart_config::{
    de::Serde,
    metadata::{SizeUnit, TimeUnit},
    ByteSize, DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::Address;

use crate::utils::{Fallback, ZERO_TO_ONE};

/// An enum that represents the version of the fee model to use.
///  - `V1`, the first model that was used in ZKsync Era. In this fee model, the pubdata price must be pegged to the L1 gas price.
///    Also, the fair L2 gas price is expected to only include the proving/computation price for the operator and not the costs that come from
///    processing the batch on L1.
///  - `V2`, the second model that was used in ZKsync Era. There the pubdata price might be independent from the L1 gas price. Also,
///    The fair L2 gas price is expected to both the proving/computation price for the operator and the costs that come from
///    processing the batch on L1.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FeeModelVersion {
    V1,
    V2,
}

impl Default for FeeModelVersion {
    fn default() -> Self {
        Self::V1
    }
}

/// Part of the state keeper configuration shared between the main and external nodes.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct SharedStateKeeperConfig {
    /// Capacity of the queue for asynchronous L2 block sealing. Once this many L2 blocks are queued,
    /// sealing will block until some of the L2 blocks from the queue are processed.
    /// 0 means that sealing is synchronous; this is mostly useful for performance comparison, testing etc.
    #[config(deprecated = "miniblock_seal_queue_capacity")]
    #[config(default_t = 10)]
    pub l2_block_seal_queue_capacity: usize,

    /// Whether to save call traces when processing blocks in the state keeper.
    #[config(default_t = true)]
    pub save_call_traces: bool,
    /// Configures whether to persist protective reads when persisting L1 batches in the state keeper.
    /// Protective reads can be written asynchronously in VM runner instead.
    /// By default, set to `false` as it is expected that a separate `vm_runner_protective_reads` component
    /// which is capable of saving protective reads is run.
    #[config(default)]
    pub protective_reads_persistence_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct SealCriteriaConfig {
    /// The max number of slots for txs in a block before it should be sealed by the slots sealer.
    #[config(default_t = 8_192)]
    pub transaction_slots: usize,
    /// Configuration option for tx to be rejected in case
    /// it takes more percentage of the block capacity than this value.
    #[config(default_t = 0.95, validate(ZERO_TO_ONE))]
    pub reject_tx_at_geometry_percentage: f64,
    /// Configuration option for tx to be rejected in case
    /// it takes more percentage of the block capacity than this value.
    #[config(default_t = 0.95, validate(ZERO_TO_ONE))]
    pub reject_tx_at_eth_params_percentage: f64,
    /// Configuration option for tx to be rejected in case
    /// it takes more percentage of the block capacity than this value.
    #[config(default_t = 0.95, validate(ZERO_TO_ONE))]
    pub reject_tx_at_gas_percentage: f64,
    /// Denotes the percentage of geometry params used in L2 block that triggers L2 block seal.
    #[config(default_t = 0.95, validate(ZERO_TO_ONE))]
    pub close_block_at_geometry_percentage: f64,
    /// Denotes the percentage of L1 params used in L2 block that triggers L2 block seal.
    #[config(default_t = 0.95, validate(ZERO_TO_ONE))]
    pub close_block_at_eth_params_percentage: f64,
    /// Denotes the percentage of L1 gas used in L2 block that triggers L2 block seal.
    #[config(default_t = 0.95, validate(ZERO_TO_ONE))]
    pub close_block_at_gas_percentage: f64,
    /// The maximum amount of pubdata that can be used by the batch.
    /// This variable should not exceed:
    /// - 128kb for calldata-based rollups
    /// - 120kb * n, where `n` is a number of blobs for blob-based rollups
    /// - the DA layer's blob size limit for the DA layer-based validiums
    /// - 100 MB for the object store-based or no-da validiums
    #[config(with = Fallback(SizeUnit::Bytes))]
    pub max_pubdata_per_batch: ByteSize,
    /// The maximum number of circuits that a batch can support.
    /// Note, that this number corresponds to the "base layer" circuits, i.e. it does not include
    /// the recursion layers' circuits.
    #[config(default_t = 31_100)]
    pub max_circuits_per_batch: usize,
}

impl SealCriteriaConfig {
    /// Creates a config object suitable for use in unit tests.
    /// Values mostly repeat the values used in the localhost environment.
    pub fn for_tests() -> Self {
        SealCriteriaConfig {
            transaction_slots: 250,
            max_pubdata_per_batch: ByteSize(100_000),
            reject_tx_at_geometry_percentage: 0.95,
            reject_tx_at_eth_params_percentage: 0.95,
            reject_tx_at_gas_percentage: 0.95,
            close_block_at_geometry_percentage: 0.95,
            close_block_at_eth_params_percentage: 0.95,
            close_block_at_gas_percentage: 0.95,
            max_circuits_per_batch: 24100,
        }
    }
}

/// State keeper config.
///
/// # Developer notes
///
/// Place here params specific for block creation (i.e., state keeper operation on the main node).
/// Params relevant to all nodes should be placed in [`SharedStateKeeperConfig`].
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct StateKeeperConfig {
    #[config(flatten)]
    pub shared: SharedStateKeeperConfig,
    #[config(flatten)]
    pub seal_criteria: SealCriteriaConfig,

    /// Deadline after which an L1 batch is going to be unconditionally sealed.
    #[config(deprecated = "block_commit_deadline")]
    #[config(default_t = Duration::from_millis(2_500))]
    pub l1_batch_commit_deadline: Duration,
    /// Deadline after which an L2 block should be sealed by the timeout sealer.
    #[config(deprecated = "miniblock_commit_deadline")]
    #[config(default_t = Duration::from_secs(1))]
    pub l2_block_commit_deadline: Duration,
    /// The max payload size threshold that triggers sealing of an L2 block.
    #[config(deprecated = "miniblock_max_payload_size")]
    #[config(default_t = ByteSize(1_000_000), with = Fallback(SizeUnit::Bytes))]
    pub l2_block_max_payload_size: ByteSize,

    /// The max amount of gas to spend on an L1 tx before its batch should be sealed by the gas sealer.
    #[config(default_t = 15_000_000)]
    pub max_single_tx_gas: u32,
    /// Max allowed gas limit for L2 transactions. Also applied on the API server.
    #[config(default_t = 15_000_000_000)]
    pub max_allowed_l2_tx_gas_limit: u64,

    // Parameters without defaults.
    /// The minimal acceptable L2 gas price, i.e. the price that should include the cost of computation/proving as well
    /// as potentially premium for congestion.
    pub minimal_l2_gas_price: u64,
    /// The constant that represents the possibility that a batch can be sealed because of overuse of computation resources.
    /// It has range from 0 to 1. If it is 0, the compute will not depend on the cost for closing the batch.
    /// If it is 1, the gas limit per batch will have to cover the entire cost of closing the batch.
    #[config(validate(ZERO_TO_ONE))]
    pub compute_overhead_part: f64,
    /// The constant that represents the possibility that a batch can be sealed because of overuse of pubdata.
    /// It has range from 0 to 1. If it is 0, the pubdata will not depend on the cost for closing the batch.
    /// If it is 1, the pubdata limit per batch will have to cover the entire cost of closing the batch.
    #[config(validate(ZERO_TO_ONE))]
    pub pubdata_overhead_part: f64,
    /// The constant amount of L1 gas that is used as the overhead for the batch. It includes the price for batch verification, etc.
    pub batch_overhead_l1_gas: u64,
    /// The maximum amount of gas that can be used by the batch. This value is derived from the circuits limitation per batch.
    pub max_gas_per_batch: u64,
    /// The version of the fee model to use.
    #[config(default_t = FeeModelVersion::V2, with = Serde![str])]
    pub fee_model_version: FeeModelVersion,
    /// Max number of computational gas that validation step is allowed to take. Also applied on the API server.
    #[config(default_t = 300_000)]
    pub validation_computational_gas_limit: u32,
    /// Allowed deployers for L2 transactions.
    #[config(nest)]
    pub deployment_allowlist: Option<DeploymentAllowlist>,
}

impl StateKeeperConfig {
    /// Creates a config object suitable for use in unit tests.
    /// Values mostly repeat the values used in the localhost environment.
    pub fn for_tests() -> Self {
        Self {
            shared: SharedStateKeeperConfig::default(),
            seal_criteria: SealCriteriaConfig::for_tests(),
            l1_batch_commit_deadline: Duration::from_millis(2500),
            l2_block_commit_deadline: Duration::from_secs(1),
            l2_block_max_payload_size: ByteSize(1_000_000),
            max_single_tx_gas: 6000000,
            max_allowed_l2_tx_gas_limit: 4000000000,
            compute_overhead_part: 0.0,
            pubdata_overhead_part: 1.0,
            batch_overhead_l1_gas: 800_000,
            max_gas_per_batch: 200_000_000,
            minimal_l2_gas_price: 100000000,
            fee_model_version: FeeModelVersion::V2,
            validation_computational_gas_limit: 300000,
            deployment_allowlist: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct CircuitBreakerConfig {
    /// Interval between circuit breaker checks.
    #[config(default_t = 2 * TimeUnit::Minutes)]
    pub sync_interval: Duration,
    /// Lag limit for the replica Postgres database if one is used. If set to `null`, the lag is unlimited,
    /// but the circuit breaker still checks that the replica DB is reachable.
    #[config(default_t = Some(Duration::from_secs(100)))]
    pub replication_lag_limit: Option<Duration>,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct MempoolConfig {
    /// Interval between syncing iterations for the mempool.
    #[config(default_t = Duration::from_millis(10))]
    pub sync_interval: Duration,
    /// Number of transactions to fetch from Postgres during a single iteration.
    #[config(default_t = 1_000)]
    pub sync_batch_size: usize,
    /// Capacity of the mempool, measured in the number of transactions.
    #[config(default_t = 10_000_000)]
    pub capacity: u64,
    /// Timeout for stuck transactions.
    #[config(default_t = 2 * TimeUnit::Days, with = Fallback(TimeUnit::Seconds))]
    pub stuck_tx_timeout: Duration,
    /// Whether to remove stuck transactions from the mempool.
    #[config(default_t = true)]
    pub remove_stuck_txs: bool,
    /// Delay interval for some mempool retrieval operations, such as retrying getting the next transaction
    /// from the pool to include into a block.
    #[config(default_t = Duration::from_millis(100), with = Fallback(TimeUnit::Millis))]
    pub delay_interval: Duration,
    /// Whether to pause inclusion of L1 (aka priority) transactions in the mempool. Should be used with care.
    #[config(default)]
    pub l1_to_l2_txs_paused: bool,
    /// Address of the initiator of high priority L2 transactions.
    #[config(default)]
    pub high_priority_l2_tx_initiator: Option<Address>,
    /// Minor version from which the high priority L2 transactions are allowed and prioritized.
    #[config(default)]
    pub high_priority_l2_tx_protocol_version: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct TimestampAsserterConfig {
    /// Minimum time between current `block.timestamp` and the end of the asserted range.
    #[config(default_t = 1 * TimeUnit::Minutes)]
    pub min_time_till_end: Duration,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(tag = "source")]
pub enum DeploymentAllowlist {
    /// Allowlist is fetched from an external source.
    #[config(alias = "Url")]
    Dynamic(DeploymentAllowlistDynamic),
    /// Allowlist is hard-coded in the config.
    Static {
        /// Allowed deployers of the new contracts.
        addresses: HashSet<Address>,
    },
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct DeploymentAllowlistDynamic {
    /// HTTP URL to fetch the file from.
    pub http_file_url: String,
    /// Refresh interval between fetches.
    #[config(default_t = 5 * TimeUnit::Minutes)]
    pub refresh_interval: Duration,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    fn expected_state_keeper_config() -> StateKeeperConfig {
        StateKeeperConfig {
            shared: SharedStateKeeperConfig {
                l2_block_seal_queue_capacity: 10,
                save_call_traces: false,
                protective_reads_persistence_enabled: true,
            },
            seal_criteria: SealCriteriaConfig {
                transaction_slots: 50,
                close_block_at_eth_params_percentage: 0.2,
                close_block_at_gas_percentage: 0.8,
                close_block_at_geometry_percentage: 0.5,
                reject_tx_at_eth_params_percentage: 0.8,
                reject_tx_at_geometry_percentage: 0.3,
                reject_tx_at_gas_percentage: 0.5,
                max_pubdata_per_batch: ByteSize(131_072),
                max_circuits_per_batch: 24100,
            },
            l1_batch_commit_deadline: Duration::from_millis(2500),
            l2_block_commit_deadline: Duration::from_millis(1000),
            l2_block_max_payload_size: ByteSize(1_000_000),
            max_single_tx_gas: 1_000_000,
            max_allowed_l2_tx_gas_limit: 2_000_000_000,
            minimal_l2_gas_price: 100000000,
            compute_overhead_part: 0.0,
            pubdata_overhead_part: 1.0,
            batch_overhead_l1_gas: 800_000,
            max_gas_per_batch: 200_000_000,
            fee_model_version: FeeModelVersion::V2,
            validation_computational_gas_limit: 10_000_000,
            deployment_allowlist: Some(DeploymentAllowlist::Dynamic(DeploymentAllowlistDynamic {
                http_file_url: "http://deployment-allowlist/".to_owned(),
                refresh_interval: Duration::from_secs(120),
            })),
        }
    }

    #[test]
    fn state_keeper_from_env() {
        let env = r#"
            CHAIN_STATE_KEEPER_TRANSACTION_SLOTS="50"
            CHAIN_STATE_KEEPER_MAX_SINGLE_TX_GAS="1000000"
            CHAIN_STATE_KEEPER_MAX_ALLOWED_L2_TX_GAS_LIMIT="2000000000"
            CHAIN_STATE_KEEPER_CLOSE_BLOCK_AT_GEOMETRY_PERCENTAGE="0.5"
            CHAIN_STATE_KEEPER_CLOSE_BLOCK_AT_GAS_PERCENTAGE="0.8"
            CHAIN_STATE_KEEPER_CLOSE_BLOCK_AT_ETH_PARAMS_PERCENTAGE="0.2"
            CHAIN_STATE_KEEPER_REJECT_TX_AT_GEOMETRY_PERCENTAGE="0.3"
            CHAIN_STATE_KEEPER_REJECT_TX_AT_ETH_PARAMS_PERCENTAGE="0.8"
            CHAIN_STATE_KEEPER_REJECT_TX_AT_GAS_PERCENTAGE="0.5"
            CHAIN_STATE_KEEPER_BLOCK_COMMIT_DEADLINE_MS="2500"
            CHAIN_STATE_KEEPER_MINIBLOCK_COMMIT_DEADLINE_MS="1000"
            CHAIN_STATE_KEEPER_MINIBLOCK_SEAL_QUEUE_CAPACITY="10"
            CHAIN_STATE_KEEPER_MINIBLOCK_MAX_PAYLOAD_SIZE="1000000"
            CHAIN_STATE_KEEPER_MINIMAL_L2_GAS_PRICE="100000000"
            CHAIN_STATE_KEEPER_COMPUTE_OVERHEAD_PART="0.0"
            CHAIN_STATE_KEEPER_PUBDATA_OVERHEAD_PART="1.0"
            CHAIN_STATE_KEEPER_BATCH_OVERHEAD_L1_GAS="800000"
            CHAIN_STATE_KEEPER_MAX_GAS_PER_BATCH="200000000"
            CHAIN_STATE_KEEPER_MAX_PUBDATA_PER_BATCH="131072"
            CHAIN_STATE_KEEPER_MAX_CIRCUITS_PER_BATCH="24100"
            CHAIN_STATE_KEEPER_FEE_MODEL_VERSION="V2"
            CHAIN_STATE_KEEPER_VALIDATION_COMPUTATIONAL_GAS_LIMIT="10000000"
            CHAIN_STATE_KEEPER_SAVE_CALL_TRACES="false"
            CHAIN_STATE_KEEPER_PROTECTIVE_READS_PERSISTENCE_ENABLED=true
            CHAIN_STATE_KEEPER_DEPLOYMENT_ALLOWLIST_SOURCE=Dynamic
            CHAIN_STATE_KEEPER_DEPLOYMENT_ALLOWLIST_HTTP_FILE_URL=http://deployment-allowlist/
            CHAIN_STATE_KEEPER_DEPLOYMENT_ALLOWLIST_REFRESH_INTERVAL=2 min
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("CHAIN_STATE_KEEPER_");
        let config: StateKeeperConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_state_keeper_config());
    }

    #[test]
    fn state_keeper_from_yaml() {
        let yaml = r#"
          transaction_slots: 50
          l1_batch_commit_deadline_ms: 2500
          l2_block_commit_deadline_ms: 1000
          l2_block_seal_queue_capacity: 10
          max_single_tx_gas: 1000000
          max_allowed_l2_tx_gas_limit: 2000000000
          reject_tx_at_geometry_percentage: 0.3
          reject_tx_at_eth_params_percentage: 0.8
          reject_tx_at_gas_percentage: 0.5
          close_block_at_geometry_percentage: 0.5
          close_block_at_eth_params_percentage: 0.2
          close_block_at_gas_percentage: 0.8
          minimal_l2_gas_price: 100000000
          compute_overhead_part: 0.0
          pubdata_overhead_part: 1.0
          batch_overhead_l1_gas: 800000
          max_gas_per_batch: 200000000
          max_pubdata_per_batch: 131072
          fee_model_version: V2
          validation_computational_gas_limit: 10000000
          save_call_traces: false
          max_circuits_per_batch: 24100
          l2_block_max_payload_size: 1000000
          protective_reads_persistence_enabled: true
          deployment_allowlist:
            source: Url
            http_file_url: http://deployment-allowlist/
            refresh_interval_secs: 120
        "#;

        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: StateKeeperConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_state_keeper_config());
    }

    #[test]
    fn state_keeper_from_idiomatic_yaml() {
        let yaml = r#"
          transaction_slots: 50
          l1_batch_commit_deadline: 2500ms
          l2_block_commit_deadline: 1 sec
          l2_block_seal_queue_capacity: 10
          max_single_tx_gas: 1000000
          max_allowed_l2_tx_gas_limit: 2000000000
          reject_tx_at_geometry_percentage: 0.3
          reject_tx_at_eth_params_percentage: 0.8
          reject_tx_at_gas_percentage: 0.5
          close_block_at_geometry_percentage: 0.5
          close_block_at_eth_params_percentage: 0.2
          close_block_at_gas_percentage: 0.8
          minimal_l2_gas_price: 100000000
          compute_overhead_part: 0.0
          pubdata_overhead_part: 1.0
          batch_overhead_l1_gas: 800000
          max_gas_per_batch: 200000000
          max_pubdata_per_batch: 128 KB
          fee_model_version: V2
          validation_computational_gas_limit: 10000000
          save_call_traces: false
          max_circuits_per_batch: 24100
          l2_block_max_payload_size: 1000000 bytes
          protective_reads_persistence_enabled: true
          deployment_allowlist:
            source: Url
            http_file_url: http://deployment-allowlist/
            refresh_interval: 2min
        "#;

        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: StateKeeperConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_state_keeper_config());
    }

    fn expected_mempool_config() -> MempoolConfig {
        MempoolConfig {
            sync_interval: Duration::from_millis(10),
            sync_batch_size: 1000,
            capacity: 1_000_000,
            stuck_tx_timeout: Duration::from_secs(10),
            remove_stuck_txs: true,
            delay_interval: Duration::from_millis(100),
            l1_to_l2_txs_paused: false,
            high_priority_l2_tx_initiator: Some(Address::from_slice(&[0x01; 20])),
            high_priority_l2_tx_protocol_version: Some(29),
        }
    }

    #[test]
    fn mempool_from_env() {
        let env = r#"
            CHAIN_MEMPOOL_SYNC_INTERVAL_MS="10"
            CHAIN_MEMPOOL_SYNC_BATCH_SIZE="1000"
            CHAIN_MEMPOOL_STUCK_TX_TIMEOUT="10"
            CHAIN_MEMPOOL_REMOVE_STUCK_TXS="true"
            CHAIN_MEMPOOL_DELAY_INTERVAL="100"
            CHAIN_MEMPOOL_CAPACITY="1000000"
            CHAIN_MEMPOOL_L1_TO_L2_TXS_PAUSED="false"
            CHAIN_MEMPOOL_HIGH_PRIORITY_L2_TX_INITIATOR="0x0101010101010101010101010101010101010101"
            CHAIN_MEMPOOL_HIGH_PRIORITY_L2_TX_PROTOCOL_VERSION="29"
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("CHAIN_MEMPOOL_");
        let config: MempoolConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_mempool_config());
    }

    #[test]
    fn mempool_from_yaml() {
        let yaml = r#"
          sync_interval_ms: 10
          sync_batch_size: 1000
          capacity: 1000000
          stuck_tx_timeout: 10
          remove_stuck_txs: true
          delay_interval: 100
          l1_to_l2_txs_paused: false
          high_priority_l2_tx_initiator: "0x0101010101010101010101010101010101010101"
          high_priority_l2_tx_protocol_version: 29
        "#;

        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: MempoolConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_mempool_config());
    }

    #[test]
    fn mempool_from_idiomatic_yaml() {
        let yaml = r#"
          sync_interval_ms: 10
          sync_batch_size: 1000
          capacity: 1000000
          stuck_tx_timeout: 10s
          remove_stuck_txs: true
          delay_interval: 100 millis
          l1_to_l2_txs_paused: false
          high_priority_l2_tx_initiator: "0x0101010101010101010101010101010101010101"
          high_priority_l2_tx_protocol_version: 29
        "#;

        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: MempoolConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_mempool_config());
    }

    fn expected_circuit_breaker_config() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            sync_interval: Duration::from_secs(1),
            replication_lag_limit: Some(Duration::from_secs(10)),
        }
    }

    #[test]
    fn circuit_breaker_from_env() {
        let env = r#"
            CHAIN_CIRCUIT_BREAKER_SYNC_INTERVAL_MS="1000"
            CHAIN_CIRCUIT_BREAKER_HTTP_REQ_MAX_RETRY_NUMBER="5"
            CHAIN_CIRCUIT_BREAKER_HTTP_REQ_RETRY_INTERVAL_SEC="2"
            CHAIN_CIRCUIT_BREAKER_REPLICATION_LAG_LIMIT_SEC="10"
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("CHAIN_CIRCUIT_BREAKER_");
        let config: CircuitBreakerConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_circuit_breaker_config());
    }

    #[test]
    fn circuit_breaker_from_yaml() {
        let yaml = r#"
          sync_interval_ms: 1000
          http_req_max_retry_number: 5
          http_req_retry_interval_sec: 2
          replication_lag_limit_sec: 10
        "#;

        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: CircuitBreakerConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_circuit_breaker_config());
    }
}
