use std::time::Duration;

use smart_config::{DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct EthProofManagerConfig {
    #[config(default_t = Duration::from_secs(10))]
    pub event_poll_interval: Duration,
    #[config(default_t = Duration::from_secs(10))]
    pub request_sending_interval: Duration,
    #[config(default_t = 1000)]
    pub event_expiration_blocks: u64,
    #[config(default_t = 2_000_000_000)]
    pub default_priority_fee_per_gas: u64,
    // todo: should be revisited
    #[config(default_t = 10)]
    pub max_reward: u64,
    #[config(default_t = Duration::from_secs(120))]
    pub proof_request_timeout: Duration,
}

impl Default for EthProofManagerConfig {
    fn default() -> Self {
        Self {
            event_poll_interval: Duration::from_secs(10),
            request_sending_interval: Duration::from_secs(10),
            event_expiration_blocks: 1000,
            default_priority_fee_per_gas: 2_000_000_000,
            max_reward: 10,
            proof_request_timeout: Duration::from_secs(120),
        }
    }
}
