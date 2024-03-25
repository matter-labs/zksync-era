use anyhow::Context as _;
use zksync_config::configs::{
    api::{
        ContractVerificationApiConfig, HealthCheckConfig, MerkleTreeApiConfig, Web3JsonRpcConfig,
    },
    ApiConfig, PrometheusConfig,
};

use crate::{envy_load, FromEnv};

impl FromEnv for ApiConfig {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            web3_json_rpc: Web3JsonRpcConfig::from_env().context("Web3JsonRpcConfig")?,
            contract_verification: ContractVerificationApiConfig::from_env()
                .context("ContractVerificationApiConfig")?,
            prometheus: PrometheusConfig::from_env().context("PrometheusConfig")?,
            healthcheck: HealthCheckConfig::from_env().context("HealthCheckConfig")?,
            merkle_tree: MerkleTreeApiConfig::from_env().context("MerkleTreeApiConfig")?,
        })
    }
}

impl FromEnv for Web3JsonRpcConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("web3_json_rpc", "API_WEB3_JSON_RPC_")
    }
}

impl FromEnv for HealthCheckConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("healthcheck", "API_HEALTHCHECK_")
    }
}

impl FromEnv for ContractVerificationApiConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("contract_verification", "API_CONTRACT_VERIFICATION_")
    }
}

impl FromEnv for MerkleTreeApiConfig {
    /// Loads configuration from env variables.
    fn from_env() -> anyhow::Result<Self> {
        envy_load("merkle_tree_api", "API_MERKLE_TREE_")
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::test_utils::{hash, EnvMutex};

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> ApiConfig {
        ApiConfig {
            web3_json_rpc: Web3JsonRpcConfig {
                http_port: 3050,
                http_url: "http://127.0.0.1:3050".into(),
                ws_port: 3051,
                ws_url: "ws://127.0.0.1:3051".into(),
                req_entities_limit: Some(10000),
                filters_disabled: false,
                filters_limit: Some(10000),
                subscriptions_limit: Some(10000),
                pubsub_polling_interval: Some(200),
                max_nonce_ahead: 5,
                request_timeout: Some(10),
                account_pks: Some(vec![
                    hash("0x0000000000000000000000000000000000000000000000000000000000000001"),
                    hash("0x0000000000000000000000000000000000000000000000000000000000000002"),
                ]),
                estimate_gas_scale_factor: 1.0f64,
                gas_price_scale_factor: 1.2,
                estimate_gas_acceptable_overestimation: 1000,
                l1_to_l2_transactions_compatibility_mode: true,
                max_tx_size: 1000000,
                vm_execution_cache_misses_limit: None,
                vm_concurrency_limit: Some(512),
                factory_deps_cache_size_mb: Some(128),
                initial_writes_cache_size_mb: Some(32),
                latest_values_cache_size_mb: Some(256),
                fee_history_limit: Some(100),
                max_batch_request_size: Some(200),
                max_response_body_size_mb: Some(10),
                websocket_requests_per_minute_limit: Some(NonZeroU32::new(10).unwrap()),
                tree_api_url: None,
                mempool_cache_update_interval: Some(50),
                mempool_cache_size: Some(10000),
            },
            contract_verification: ContractVerificationApiConfig {
                port: 3070,
                url: "http://127.0.0.1:3070".into(),
            },
            prometheus: PrometheusConfig {
                listener_port: 3312,
                pushgateway_url: "http://127.0.0.1:9091".into(),
                push_interval_ms: Some(100),
            },
            healthcheck: HealthCheckConfig {
                port: 8081,
                slow_time_limit_ms: Some(250),
                hard_time_limit_ms: Some(2_000),
            },
            merkle_tree: MerkleTreeApiConfig { port: 8082 },
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            API_WEB3_JSON_RPC_HTTP_PORT="3050"
            API_WEB3_JSON_RPC_HTTP_URL="http://127.0.0.1:3050"
            API_WEB3_JSON_RPC_WS_PORT="3051"
            API_WEB3_JSON_RPC_WS_URL="ws://127.0.0.1:3051"
            API_WEB3_JSON_RPC_REQ_ENTITIES_LIMIT=10000
            API_WEB3_JSON_RPC_FILTERS_DISABLED=false
            API_WEB3_JSON_RPC_FILTERS_LIMIT=10000
            API_WEB3_JSON_RPC_SUBSCRIPTIONS_LIMIT=10000
            API_WEB3_JSON_RPC_PUBSUB_POLLING_INTERVAL=200
            API_WEB3_JSON_RPC_MAX_NONCE_AHEAD=5
            API_WEB3_JSON_RPC_GAS_PRICE_SCALE_FACTOR=1.2
            API_WEB3_JSON_RPC_REQUEST_TIMEOUT=10
            API_WEB3_JSON_RPC_ACCOUNT_PKS="0x0000000000000000000000000000000000000000000000000000000000000001,0x0000000000000000000000000000000000000000000000000000000000000002"
            API_WEB3_JSON_RPC_ESTIMATE_GAS_SCALE_FACTOR=1.0
            API_WEB3_JSON_RPC_ESTIMATE_GAS_ACCEPTABLE_OVERESTIMATION=1000
            API_WEB3_JSON_RPC_L1_TO_L2_TRANSACTIONS_COMPATIBILITY_MODE=true
            API_WEB3_JSON_RPC_MAX_TX_SIZE=1000000
            API_WEB3_JSON_RPC_VM_CONCURRENCY_LIMIT=512
            API_WEB3_JSON_RPC_FACTORY_DEPS_CACHE_SIZE_MB=128
            API_WEB3_JSON_RPC_INITIAL_WRITES_CACHE_SIZE_MB=32
            API_WEB3_JSON_RPC_LATEST_VALUES_CACHE_SIZE_MB=256
            API_WEB3_JSON_RPC_FEE_HISTORY_LIMIT=100
            API_WEB3_JSON_RPC_MAX_BATCH_REQUEST_SIZE=200
            API_WEB3_JSON_RPC_WEBSOCKET_REQUESTS_PER_MINUTE_LIMIT=10
            API_WEB3_JSON_RPC_MEMPOOL_CACHE_SIZE=10000
            API_WEB3_JSON_RPC_MEMPOOL_CACHE_UPDATE_INTERVAL=50
            API_CONTRACT_VERIFICATION_PORT="3070"
            API_CONTRACT_VERIFICATION_URL="http://127.0.0.1:3070"
            API_WEB3_JSON_RPC_MAX_RESPONSE_BODY_SIZE_MB=10
            API_PROMETHEUS_LISTENER_PORT="3312"
            API_PROMETHEUS_PUSHGATEWAY_URL="http://127.0.0.1:9091"
            API_PROMETHEUS_PUSH_INTERVAL_MS=100
            API_HEALTHCHECK_PORT=8081
            API_HEALTHCHECK_SLOW_TIME_LIMIT_MS=250
            API_HEALTHCHECK_HARD_TIME_LIMIT_MS=2000
            API_MERKLE_TREE_PORT=8082
        "#;
        lock.set_env(config);

        let actual = ApiConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
