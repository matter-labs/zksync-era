use std::time::Duration;

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct FriProverGatewayConfig {
    pub api_url: String,
    #[config(default_t = Duration::from_secs(1000), with = TimeUnit::Seconds)]
    pub api_poll_duration_secs: Duration,
    // Configurations for prometheus
    #[config(default_t = 3314)]
    pub prometheus_listener_port: u16,
}
