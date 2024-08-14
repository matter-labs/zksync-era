use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct FriProverGatewayConfig {
    pub api_url: String,
    pub api_poll_duration_secs: u16,

    /// Configurations for prometheus
    pub prometheus_listener_port: u16,
    pub prometheus_pushgateway_url: String,
    pub prometheus_push_interval_ms: Option<u64>,
}

impl FriProverGatewayConfig {
    pub fn api_poll_duration(&self) -> Duration {
        Duration::from_secs(self.api_poll_duration_secs as u64)
    }
}
