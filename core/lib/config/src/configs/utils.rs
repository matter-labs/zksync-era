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
    /// Push interval.
    #[config(default_t = Duration::from_millis(100))]
    pub push_interval: Duration,
}
