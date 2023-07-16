use crate::configs::envy_load;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct PrometheusConfig {
    /// Port to which the Prometheus exporter server is listening.
    pub listener_port: u16,
    /// Url of Pushgateway.
    pub pushgateway_url: String,
    /// Push interval in ms.
    pub push_interval_ms: Option<u64>,
}

impl PrometheusConfig {
    pub fn from_env() -> Self {
        envy_load("prometheus", "API_PROMETHEUS_")
    }
    pub fn push_interval(&self) -> Duration {
        Duration::from_millis(self.push_interval_ms.unwrap_or(100))
    }
}
