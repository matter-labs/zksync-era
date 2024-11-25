use std::time::Duration;

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

/// Configuration for the housekeeper.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct HouseKeeperConfig {
    #[config(default_t = Duration::from_secs(10), with = TimeUnit::Millis)]
    pub l1_batch_metrics_reporting_interval_ms: Duration,
}
