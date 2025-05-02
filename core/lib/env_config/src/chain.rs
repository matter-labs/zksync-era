use zksync_config::configs::chain::{
    CircuitBreakerConfig, MempoolConfig, OperationsManagerConfig, StateKeeperConfig,
};

use crate::{envy_load, FromEnv};

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
    use zksync_basic_types::commitment::L1BatchCommitmentMode;
    use zksync_config::configs::chain::FeeModelVersion;

    use super::*;
    use crate::test_utils::{addr, hash, EnvMutex};

    static MUTEX: EnvMutex = EnvMutex::new();
    const VALIDIUM_L1_BATCH_COMMIT_DATA_GENERATOR_MODE: &str = "Validium";
    const ROLLUP_L1_BATCH_COMMIT_DATA_GENERATOR_MODE: &str = "Rollup";

    #[allow(deprecated)]
    fn expected_state_keeper_config(
        l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    ) -> StateKeeperConfig {
        StateKeeperConfig {
            transaction_slots: 50,
            block_commit_deadline_ms: 2500,
            l2_block_commit_deadline_ms: 1000,
            l2_block_seal_queue_capacity: 10,
            l2_block_max_payload_size: 1_000_000,
            max_single_tx_gas: 1_000_000,
            max_allowed_l2_tx_gas_limit: 2_000_000_000,
            close_block_at_eth_params_percentage: 0.2,
            close_block_at_gas_percentage: 0.8,
            close_block_at_geometry_percentage: 0.5,
            reject_tx_at_eth_params_percentage: 0.8,
            reject_tx_at_geometry_percentage: 0.3,
            fee_account_addr: Some(addr("de03a0B5963f75f1C8485B355fF6D30f3093BDE7")),
            reject_tx_at_gas_percentage: 0.5,
            minimal_l2_gas_price: 100000000,
            compute_overhead_part: 0.0,
            pubdata_overhead_part: 1.0,
            batch_overhead_l1_gas: 800_000,
            max_gas_per_batch: 200_000_000,
            max_pubdata_per_batch: 100_000,
            fee_model_version: FeeModelVersion::V2,
            validation_computational_gas_limit: 10_000_000,
            save_call_traces: false,
            bootloader_hash: Some(hash(
                "0x010007ede999d096c84553fb514d3d6ca76fbf39789dda76bfeda9f3ae06236e",
            )),
            default_aa_hash: Some(hash(
                "0x0100055b041eb28aff6e3a6e0f37c31fd053fc9ef142683b05e5f0aee6934066",
            )),
            evm_emulator_hash: None,
            l1_batch_commit_data_generator_mode,
            max_circuits_per_batch: 24100,
            protective_reads_persistence_enabled: true,
            deployment_allowlist: None,
        }
    }

    fn state_keeper_config(l1_batch_commit_data_generator_mode: &str) -> String {
        format!(
            r#"
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
            CHAIN_STATE_KEEPER_MINIBLOCK_MAX_PAYLOAD_SIZE="1000000"
            CHAIN_STATE_KEEPER_MINIMAL_L2_GAS_PRICE="100000000"
            CHAIN_STATE_KEEPER_COMPUTE_OVERHEAD_PART="0.0"
            CHAIN_STATE_KEEPER_PUBDATA_OVERHEAD_PART="1.0"
            CHAIN_STATE_KEEPER_BATCH_OVERHEAD_L1_GAS="800000"
            CHAIN_STATE_KEEPER_MAX_GAS_PER_BATCH="200000000"
            CHAIN_STATE_KEEPER_MAX_PUBDATA_PER_BATCH="100000"
            CHAIN_STATE_KEEPER_MAX_CIRCUITS_PER_BATCH="24100"
            CHAIN_STATE_KEEPER_FEE_MODEL_VERSION="V2"
            CHAIN_STATE_KEEPER_VALIDATION_COMPUTATIONAL_GAS_LIMIT="10000000"
            CHAIN_STATE_KEEPER_SAVE_CALL_TRACES="false"
            CHAIN_STATE_KEEPER_BOOTLOADER_HASH=0x010007ede999d096c84553fb514d3d6ca76fbf39789dda76bfeda9f3ae06236e
            CHAIN_STATE_KEEPER_DEFAULT_AA_HASH=0x0100055b041eb28aff6e3a6e0f37c31fd053fc9ef142683b05e5f0aee6934066
            CHAIN_STATE_KEEPER_PROTECTIVE_READS_PERSISTENCE_ENABLED=true
            CHAIN_STATE_KEEPER_L1_BATCH_COMMIT_DATA_GENERATOR_MODE="{l1_batch_commit_data_generator_mode}"
        "#
        )
    }

    fn _state_keeper_from_env(config: &str, expected_config: StateKeeperConfig) {
        let mut lock = MUTEX.lock();
        lock.set_env(config);

        let actual = StateKeeperConfig::from_env().unwrap();
        assert_eq!(actual, expected_config);
    }

    #[test]
    fn state_keeper_from_env() {
        _state_keeper_from_env(
            &state_keeper_config(ROLLUP_L1_BATCH_COMMIT_DATA_GENERATOR_MODE),
            expected_state_keeper_config(L1BatchCommitmentMode::Rollup),
        );
        _state_keeper_from_env(
            &state_keeper_config(VALIDIUM_L1_BATCH_COMMIT_DATA_GENERATOR_MODE),
            expected_state_keeper_config(L1BatchCommitmentMode::Validium),
        );
    }

    fn expected_mempool_config() -> MempoolConfig {
        MempoolConfig {
            sync_interval_ms: 10,
            sync_batch_size: 1000,
            capacity: 1_000_000,
            stuck_tx_timeout: 10,
            remove_stuck_txs: true,
            delay_interval: 100,
            skip_unsafe_deposit_checks: false,
            l1_to_l2_txs_paused: true,
        }
    }

    #[test]
    fn mempool_from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            CHAIN_MEMPOOL_SYNC_INTERVAL_MS="10"
            CHAIN_MEMPOOL_SYNC_BATCH_SIZE="1000"
            CHAIN_MEMPOOL_STUCK_TX_TIMEOUT="10"
            CHAIN_MEMPOOL_REMOVE_STUCK_TXS="true"
            CHAIN_MEMPOOL_DELAY_INTERVAL="100"
            CHAIN_MEMPOOL_CAPACITY="1000000"
            CHAIN_MEMPOOL_L1_TO_L2_TXS_PAUSED="true"
        "#;
        lock.set_env(config);

        let actual = MempoolConfig::from_env().unwrap();
        assert_eq!(actual, expected_mempool_config());
    }

    fn expected_circuit_breaker_config() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            sync_interval_ms: 1000,
            http_req_max_retry_number: 5,
            http_req_retry_interval_sec: 2,
            replication_lag_limit_sec: Some(10),
        }
    }

    #[test]
    fn circuit_breaker_from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            CHAIN_CIRCUIT_BREAKER_SYNC_INTERVAL_MS="1000"
            CHAIN_CIRCUIT_BREAKER_HTTP_REQ_MAX_RETRY_NUMBER="5"
            CHAIN_CIRCUIT_BREAKER_HTTP_REQ_RETRY_INTERVAL_SEC="2"
            CHAIN_CIRCUIT_BREAKER_REPLICATION_LAG_LIMIT_SEC="10"
        "#;
        lock.set_env(config);

        let actual = CircuitBreakerConfig::from_env().unwrap();
        assert_eq!(actual, expected_circuit_breaker_config());
    }

    #[test]
    #[allow(deprecated)]
    fn default_state_keeper_mode() {
        assert_eq!(
            StateKeeperConfig::default().l1_batch_commit_data_generator_mode,
            L1BatchCommitmentMode::Rollup
        );
    }
}
