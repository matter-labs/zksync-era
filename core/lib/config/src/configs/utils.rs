use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Prometheus {
    /// Port to which the Prometheus exporter server is listening.
    pub listener_port: u16,
    /// Url of Pushgateway.
    pub pushgateway_url: String,
    /// Push interval in ms.
    pub push_interval_ms: Option<u64>,
}

impl Prometheus {
    pub fn push_interval(&self) -> Duration {
        Duration::from_millis(self.push_interval_ms.unwrap_or(100))
    }
}
