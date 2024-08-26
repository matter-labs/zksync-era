use zksync_config::configs::BaseTokenAdjusterConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for BaseTokenAdjusterConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("base_token_adjuster", "BASE_TOKEN_ADJUSTER_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> BaseTokenAdjusterConfig {
        BaseTokenAdjusterConfig {
            price_polling_interval_ms: 10_000,
            price_cache_update_interval_ms: 11_000,
            max_tx_gas: 1_000_000,
            default_priority_fee_per_gas: 50_000,
            max_acceptable_priority_fee_in_gwei: 10_000_000_000,
            l1_receipt_checking_max_attempts: 5,
            l1_receipt_checking_sleep_ms: 20_000,
            l1_tx_sending_max_attempts: 10,
            l1_tx_sending_sleep_ms: 30_000,
            price_fetching_max_attempts: 20,
            price_fetching_sleep_ms: 10_000,
            halt_on_error: true,
        }
    }

    fn expected_config_with_defaults() -> BaseTokenAdjusterConfig {
        BaseTokenAdjusterConfig {
            price_polling_interval_ms: 30_000,
            price_cache_update_interval_ms: 500,
            max_tx_gas: 80_000,
            default_priority_fee_per_gas: 1_000_000_000,
            max_acceptable_priority_fee_in_gwei: 100_000_000_000,
            l1_receipt_checking_max_attempts: 3,
            l1_receipt_checking_sleep_ms: 30_000,
            l1_tx_sending_max_attempts: 3,
            l1_tx_sending_sleep_ms: 30_000,
            price_fetching_max_attempts: 3,
            price_fetching_sleep_ms: 5_000,
            halt_on_error: false,
        }
    }

    #[test]
    fn from_env_base_token_adjuster() {
        let mut lock = MUTEX.lock();
        let config = r#"
            BASE_TOKEN_ADJUSTER_PRICE_POLLING_INTERVAL_MS=10000
            BASE_TOKEN_ADJUSTER_PRICE_CACHE_UPDATE_INTERVAL_MS=11000
            BASE_TOKEN_ADJUSTER_MAX_TX_GAS=1000000
            BASE_TOKEN_ADJUSTER_DEFAULT_PRIORITY_FEE_PER_GAS=50000
            BASE_TOKEN_ADJUSTER_MAX_ACCEPTABLE_PRIORITY_FEE_IN_GWEI=10000000000
            BASE_TOKEN_ADJUSTER_L1_RECEIPT_CHECKING_MAX_ATTEMPTS=5
            BASE_TOKEN_ADJUSTER_L1_RECEIPT_CHECKING_SLEEP_MS=20000
            BASE_TOKEN_ADJUSTER_L1_TX_SENDING_MAX_ATTEMPTS=10
            BASE_TOKEN_ADJUSTER_L1_TX_SENDING_SLEEP_MS=30000
            BASE_TOKEN_ADJUSTER_PRICE_FETCHING_MAX_ATTEMPTS=20
            BASE_TOKEN_ADJUSTER_PRICE_FETCHING_SLEEP_MS=10000
            BASE_TOKEN_ADJUSTER_HALT_ON_ERROR=true
        "#;
        lock.set_env(config);

        let actual = BaseTokenAdjusterConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }

    #[test]
    fn from_env_base_token_adjuster_defaults() {
        let mut lock = MUTEX.lock();
        lock.remove_env(&[
            "BASE_TOKEN_ADJUSTER_PRICE_POLLING_INTERVAL_MS",
            "BASE_TOKEN_ADJUSTER_PRICE_CACHE_UPDATE_INTERVAL_MS",
            "BASE_TOKEN_ADJUSTER_MAX_TX_GAS",
            "BASE_TOKEN_ADJUSTER_DEFAULT_PRIORITY_FEE_PER_GAS",
            "BASE_TOKEN_ADJUSTER_MAX_ACCEPTABLE_PRIORITY_FEE_IN_GWEI",
            "BASE_TOKEN_ADJUSTER_L1_RECEIPT_CHECKING_MAX_ATTEMPTS",
            "BASE_TOKEN_ADJUSTER_L1_RECEIPT_CHECKING_SLEEP_MS",
            "BASE_TOKEN_ADJUSTER_L1_TX_SENDING_MAX_ATTEMPTS",
            "BASE_TOKEN_ADJUSTER_L1_TX_SENDING_SLEEP_MS",
            "BASE_TOKEN_ADJUSTER_PRICE_FETCHING_MAX_ATTEMPTS",
            "BASE_TOKEN_ADJUSTER_PRICE_FETCHING_SLEEP_MS",
            "BASE_TOKEN_ADJUSTER_HALT_ON_ERROR",
        ]);

        let actual = BaseTokenAdjusterConfig::from_env().unwrap();
        assert_eq!(actual, expected_config_with_defaults());
    }
}
