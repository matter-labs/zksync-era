use serde::Deserialize;

/// Configuration for the house keeper.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct HouseKeeperConfig {
    pub l1_batch_metrics_reporting_interval_ms: u64,
}
