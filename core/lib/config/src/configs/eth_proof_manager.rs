use std::time::Duration;

use smart_config::{DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct EthProofManagerConfig {
    #[config(default_t = Duration::from_secs(10))]
    pub event_poll_interval: Duration,
}

impl Default for EthProofManagerConfig {
    fn default() -> Self {
        Self {
            event_poll_interval: Duration::from_secs(10),
        }
    }
}
