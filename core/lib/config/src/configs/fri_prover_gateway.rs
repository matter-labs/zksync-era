use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct FriProverGatewayConfig {
    pub api_url: String,
    pub api_poll_duration_secs: u16,

    /// Configurations for prometheus
    pub prometheus_listener_port: u16,
    pub prometheus_pushgateway_url: String,
    pub prometheus_push_interval_ms: Option<u64>,
    #[serde(default)]
    pub api_mode: ApiMode,
    #[serde(default)]
    pub port: Option<u16>,
}

impl FriProverGatewayConfig {
    pub fn api_poll_duration(&self) -> Duration {
        Duration::from_secs(self.api_poll_duration_secs as u64)
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
pub enum ApiMode {
    /// The legacy API mode, which is compatible with the old prover API.
    #[default]
    Legacy,
    /// The new API mode, which is compatible with the prover cluster API.
    ProverCluster,
}
