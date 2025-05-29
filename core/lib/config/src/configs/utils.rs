use std::time::Duration;

use smart_config::{DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct PrometheusConfig {
    /// Port to which the Prometheus exporter server is listening.
    #[config(alias = "port")]
    pub listener_port: Option<u16>,
    /// URL of the push gateway.
    pub pushgateway_url: Option<String>,
    /// Push interval in ms.
    pub push_interval_ms: Option<u64>,
}

impl PrometheusConfig {
    pub fn push_interval(&self) -> Duration {
        Duration::from_millis(self.push_interval_ms.unwrap_or(100))
    }
}
