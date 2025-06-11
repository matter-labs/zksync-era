use std::time::Duration;

use smart_config::{DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct EthProofManagerConfig {
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
    #[config(default_t = Duration::from_secs(30))]
    pub l1_receipt_checking_sleep: Duration,
    /// Maximum number of attempts to submit L1 transaction before failing over
    #[config(default_t = 5)]
    pub l1_tx_sending_max_attempts: u32,
    /// Number of seconds to sleep between the transaction sending attempts
    #[config(default_t = Duration::from_secs(30))]
    pub l1_tx_sending_sleep: Duration,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    fn expected_config() -> EthProofManagerConfig {
        EthProofManagerConfig {
            max_tx_gas: 1_000_000,
            default_priority_fee_per_gas: 50_000,
            max_acceptable_priority_fee_in_gwei: 10_000_000_000,
            l1_receipt_checking_max_attempts: 5,
            l1_receipt_checking_sleep: Duration::from_secs(20),
            l1_tx_sending_max_attempts: 10,
            l1_tx_sending_sleep: Duration::from_secs(30),
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            ETH_PROOF_MANAGER_MAX_TX_GAS=1000000
            ETH_PROOF_MANAGER_DEFAULT_PRIORITY_FEE_PER_GAS=50000
            ETH_PROOF_MANAGER_L1_RECEIPT_CHECKING_MAX_ATTEMPTS=5
            ETH_PROOF_MANAGER_L1_RECEIPT_CHECKING_SLEEP_MS=20000
            ETH_PROOF_MANAGER_L1_TX_SENDING_MAX_ATTEMPTS=10
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("ETH_PROOF_MANAGER_");

        let config: EthProofManagerConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          max_tx_gas: 1000000
          default_priority_fee_per_gas: 50000
          max_acceptable_priority_fee_in_gwei: 10000000000
          l1_receipt_checking_sleep_ms: 20000
          l1_receipt_checking_max_attempts: 5
          l1_tx_sending_max_attempts: 10
          l1_tx_sending_sleep_ms: 30000
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: EthProofManagerConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
