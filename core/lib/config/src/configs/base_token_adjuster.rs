use std::time::Duration;

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct BaseTokenAdjusterConfig {
    /// How often to spark a new cycle of the ratio persister to fetch external prices and persis ratios.
    #[config(default_t = Duration::from_secs(30), with = TimeUnit::Millis)]
    pub price_polling_interval_ms: Duration,

    /// We (in memory) cache the ratio fetched from db. This interval defines frequency of refetch from db.
    #[config(default_t = Duration::from_millis(500), with = TimeUnit::Millis)]
    pub price_cache_update_interval_ms: Duration,

    /// Max amount of gas that L1 base token update can consume per transaction
    #[config(default_t = 80_000)]
    pub max_tx_gas: u64,

    /// Default priority fee per gas used to instantiate the signing client
    #[config(default_t = 1_000_000_000)]
    pub default_priority_fee_per_gas: u64,

    /// Maximum acceptable priority fee in gwei to prevent sending transaction with extremely high priority fee.
    #[config(default_t = 100_000_000_000)]
    pub max_acceptable_priority_fee_in_gwei: u64,

    /// Maximum number of attempts to get L1 transaction receipt before failing over
    #[config(default_t = 3)]
    pub l1_receipt_checking_max_attempts: u32,

    /// Number of seconds to sleep between the receipt checking attempts
    #[config(default_t = Duration::from_secs(30), with = TimeUnit::Millis)]
    pub l1_receipt_checking_sleep_ms: Duration,

    /// Maximum number of attempts to submit L1 transaction before failing over
    #[config(default_t = 3)]
    pub l1_tx_sending_max_attempts: u32,

    /// Number of seconds to sleep between the transaction sending attempts
    #[config(default_t = Duration::from_secs(30), with = TimeUnit::Millis)]
    pub l1_tx_sending_sleep_ms: Duration,

    /// How many percent a quote needs to change in order for update to be propagated to L1.
    /// Exists to save on gas.
    #[config(default_t = 10)]
    pub l1_update_deviation_percentage: u32,

    /// Maximum number of attempts to fetch quote from a remote API before failing over
    #[config(default_t = 3)]
    pub price_fetching_max_attempts: u32,

    /// Number of seconds to sleep between price fetching attempts
    #[config(default_t = Duration::from_secs(5), with = TimeUnit::Millis)]
    pub price_fetching_sleep_ms: Duration,

    /// Defines whether base_token_adjuster should halt the process if there was an error while
    /// fetching or persisting the quote. Generally that should be set to false to not to halt
    /// the server process if an external api is not available or if L1 is congested.
    #[config(default_t = false)]
    pub halt_on_error: bool,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    fn expected_config() -> BaseTokenAdjusterConfig {
        BaseTokenAdjusterConfig {
            price_polling_interval_ms: Duration::from_secs(10),
            price_cache_update_interval_ms: Duration::from_secs(11),
            max_tx_gas: 1_000_000,
            default_priority_fee_per_gas: 50_000,
            max_acceptable_priority_fee_in_gwei: 10_000_000_000,
            l1_receipt_checking_max_attempts: 5,
            l1_receipt_checking_sleep_ms: Duration::from_secs(20),
            l1_tx_sending_max_attempts: 10,
            l1_tx_sending_sleep_ms: Duration::from_secs(30),
            price_fetching_max_attempts: 20,
            price_fetching_sleep_ms: Duration::from_secs(10),
            l1_update_deviation_percentage: 20,
            halt_on_error: true,
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            BASE_TOKEN_ADJUSTER_PRICE_POLLING_INTERVAL_MS=10000
            BASE_TOKEN_ADJUSTER_PRICE_CACHE_UPDATE_INTERVAL_MS=11000
            BASE_TOKEN_ADJUSTER_MAX_TX_GAS=1000000
            BASE_TOKEN_ADJUSTER_DEFAULT_PRIORITY_FEE_PER_GAS=50000
            BASE_TOKEN_ADJUSTER_MAX_ACCEPTABLE_PRIORITY_FEE_IN_GWEI=10000000000
            BASE_TOKEN_ADJUSTER_L1_RECEIPT_CHECKING_MAX_ATTEMPTS=5
            BASE_TOKEN_ADJUSTER_L1_RECEIPT_CHECKING_SLEEP_MS=20000
            BASE_TOKEN_ADJUSTER_L1_TX_SENDING_MAX_ATTEMPTS=10
            BASE_TOKEN_ADJUSTER_L1_TX_SENDING_SLEEP_MS=30000
            BASE_TOKEN_ADJUSTER_L1_UPDATE_DEVIATION_PERCENTAGE=20
            BASE_TOKEN_ADJUSTER_PRICE_FETCHING_MAX_ATTEMPTS=20
            BASE_TOKEN_ADJUSTER_PRICE_FETCHING_SLEEP_MS=10000
            BASE_TOKEN_ADJUSTER_HALT_ON_ERROR=true
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("BASE_TOKEN_ADJUSTER_");

        let config: BaseTokenAdjusterConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          price_polling_interval_ms: 10000
          price_cache_update_interval_ms: 11000
          max_tx_gas: 1000000
          default_priority_fee_per_gas: 50000
          max_acceptable_priority_fee_in_gwei: 10000000000
          l1_receipt_checking_sleep_ms: 20000
          l1_receipt_checking_max_attempts: 5
          l1_tx_sending_max_attempts: 10
          l1_tx_sending_sleep_ms: 30000
          halt_on_error: true
          price_fetching_max_attempts: 20
          price_fetching_sleep_ms: 10000
          l1_update_deviation_percentage: 20
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: BaseTokenAdjusterConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
