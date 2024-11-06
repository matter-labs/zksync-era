use serde::Deserialize;

/// Configuration for the house keeper.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct HouseKeeperConfig {
    pub l1_batch_metrics_reporting_interval_ms: u64,
    pub database_health_polling_interval_ms: u64,
    pub eth_sender_health_polling_interval_ms: u64,
    pub state_keeper_health_polling_interval_ms: u64,
}
