use crate::{envy_load, FromEnv};
use anyhow::Context as _;
use zksync_config::configs::chain::{
    ChainConfig, CircuitBreakerConfig, MempoolConfig, NetworkConfig, OperationsManagerConfig,
    StateKeeperConfig,
};

impl FromEnv for ChainConfig {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            network: NetworkConfig::from_env().context("NetworkConfig")?,
            state_keeper: StateKeeperConfig::from_env().context("StateKeeperConfig")?,
            operations_manager: OperationsManagerConfig::from_env()
                .context("OperationsManagerConfig")?,
            mempool: MempoolConfig::from_env().context("MempoolConfig")?,
            circuit_breaker: CircuitBreakerConfig::from_env().context("CircuitBreakerConfig")?,
        })
    }
}

impl FromEnv for NetworkConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("network", "CHAIN_ETH_")
    }
}

impl FromEnv for StateKeeperConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("state_keeper", "CHAIN_STATE_KEEPER_")
    }
}

impl FromEnv for OperationsManagerConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("operations_manager", "CHAIN_OPERATIONS_MANAGER_")
    }
}

impl FromEnv for CircuitBreakerConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("circuit_breaker", "CHAIN_CIRCUIT_BREAKER_")
    }
}

impl FromEnv for MempoolConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("mempool", "CHAIN_MEMPOOL_")
    }
}

#[cfg(test)]
mod tests {
    use zksync_basic_types::L2ChainId;

    use super::*;
    use crate::test_utils::{addr, EnvMutex};

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> ChainConfig {
        ChainConfig {
            network: NetworkConfig {
                network: "localhost".parse().unwrap(),
                zksync_network: "localhost".to_string(),
                zksync_network_id: L2ChainId::from(270),
            },
            state_keeper: StateKeeperConfig {
                transaction_slots: 50,
                block_commit_deadline_ms: 2500,
                miniblock_commit_deadline_ms: 1000,
                miniblock_seal_queue_capacity: 10,
                max_single_tx_gas: 1_000_000,
                max_allowed_l2_tx_gas_limit: 2_000_000_000,
                close_block_at_eth_params_percentage: 0.2,
                close_block_at_gas_percentage: 0.8,
                close_block_at_geometry_percentage: 0.5,
                reject_tx_at_eth_params_percentage: 0.8,
                reject_tx_at_geometry_percentage: 0.3,
                fee_account_addr: addr("de03a0B5963f75f1C8485B355fF6D30f3093BDE7"),
                reject_tx_at_gas_percentage: 0.5,
                fair_l2_gas_price: 250000000,
                validation_computational_gas_limit: 10_000_000,
                save_call_traces: false,
                virtual_blocks_interval: 1,
                virtual_blocks_per_miniblock: 1,
                upload_witness_inputs_to_gcs: false,
                enum_index_migration_chunk_size: Some(2_000),
            },
            operations_manager: OperationsManagerConfig {
                delay_interval: 100,
            },
            mempool: MempoolConfig {
                sync_interval_ms: 10,
                sync_batch_size: 1000,
                capacity: 1_000_000,
                stuck_tx_timeout: 10,
                remove_stuck_txs: true,
                delay_interval: 100,
            },
            circuit_breaker: CircuitBreakerConfig {
                sync_interval_ms: 1000,
                http_req_max_retry_number: 5,
                http_req_retry_interval_sec: 2,
                replication_lag_limit_sec: Some(10),
            },
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            CHAIN_ETH_NETWORK="localhost"
            CHAIN_ETH_ZKSYNC_NETWORK="localhost"
            CHAIN_ETH_ZKSYNC_NETWORK_ID=270
            CHAIN_STATE_KEEPER_TRANSACTION_SLOTS="50"
            CHAIN_STATE_KEEPER_FEE_ACCOUNT_ADDR="0xde03a0B5963f75f1C8485B355fF6D30f3093BDE7"
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
            CHAIN_STATE_KEEPER_FAIR_L2_GAS_PRICE="250000000"
            CHAIN_STATE_KEEPER_VALIDATION_COMPUTATIONAL_GAS_LIMIT="10000000"
            CHAIN_STATE_KEEPER_SAVE_CALL_TRACES="false"
            CHAIN_STATE_KEEPER_UPLOAD_WITNESS_INPUTS_TO_GCS="false"
            CHAIN_STATE_KEEPER_ENUM_INDEX_MIGRATION_CHUNK_SIZE="2000"
            CHAIN_OPERATIONS_MANAGER_DELAY_INTERVAL="100"
            CHAIN_MEMPOOL_SYNC_INTERVAL_MS="10"
            CHAIN_MEMPOOL_SYNC_BATCH_SIZE="1000"
            CHAIN_MEMPOOL_STUCK_TX_TIMEOUT="10"
            CHAIN_MEMPOOL_REMOVE_STUCK_TXS="true"
            CHAIN_MEMPOOL_DELAY_INTERVAL="100"
            CHAIN_MEMPOOL_CAPACITY="1000000"
            CHAIN_CIRCUIT_BREAKER_SYNC_INTERVAL_MS="1000"
            CHAIN_CIRCUIT_BREAKER_HTTP_REQ_MAX_RETRY_NUMBER="5"
            CHAIN_CIRCUIT_BREAKER_HTTP_REQ_RETRY_INTERVAL_SEC="2"
            CHAIN_CIRCUIT_BREAKER_REPLICATION_LAG_LIMIT_SEC="10"
        "#;
        lock.set_env(config);

        let actual = ChainConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
