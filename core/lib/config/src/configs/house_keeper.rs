use std::time::Duration;

use smart_config::{DescribeConfig, DeserializeConfig};

/// Configuration for the housekeeper.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct HouseKeeperConfig {
    #[config(default_t = Duration::from_secs(10))]
    pub l1_batch_metrics_reporting_interval: Duration,
    /// How often to check whether the oldest committed-but-unexecuted batch has enough 2FA
    /// approvals (`EraMultisigValidator.getApprovals` vs `threshold`). A relaxed default is used
    /// since this involves L1 RPC calls and the metric is meant to feed an hour-scale alert.
    #[config(default_t = Duration::from_secs(60))]
    pub two_factor_approval_reporting_interval: Duration,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    fn expected_config() -> HouseKeeperConfig {
        HouseKeeperConfig {
            l1_batch_metrics_reporting_interval: Duration::from_secs(10),
            two_factor_approval_reporting_interval: Duration::from_secs(60),
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            HOUSE_KEEPER_L1_BATCH_METRICS_REPORTING_INTERVAL_MS="10000"
            HOUSE_KEEPER_TWO_FACTOR_APPROVAL_REPORTING_INTERVAL_MS="60000"
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("HOUSE_KEEPER_");

        let config: HouseKeeperConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          l1_batch_metrics_reporting_interval_ms: 10000
          two_factor_approval_reporting_interval_ms: 60000
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: HouseKeeperConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
