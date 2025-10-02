use std::time::Duration;

use smart_config::{DescribeConfig, DeserializeConfig};
#[cfg(feature = "observability_ext")]
use zksync_vlog::prometheus::PrometheusExporterConfig;

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct PrometheusConfig {
    /// Port to which the Prometheus exporter server is listening.
    #[config(alias = "port")]
    pub listener_port: Option<u16>,
    /// URL of the push gateway.
    pub pushgateway_url: Option<String>,
    /// Push interval to the push gateway.
    #[config(default_t = Duration::from_millis(100))]
    pub push_interval: Duration,
}

impl PrometheusConfig {
    /// Builds a Prometheus exporter configuration trying to use the provided `prometheus_port` or fallback to `self`.
    #[cfg(feature = "observability_ext")]
    pub fn build_exporter_config(
        &self,
        prometheus_port: Option<u16>,
    ) -> Option<PrometheusExporterConfig> {
        if let Some(port) = prometheus_port {
            Some(PrometheusExporterConfig::pull(port))
        } else if let Some(base_url) = &self.pushgateway_url {
            let url = PrometheusExporterConfig::gateway_endpoint(base_url);
            Some(PrometheusExporterConfig::push(url, self.push_interval))
        } else if let Some(port) = self.listener_port {
            Some(PrometheusExporterConfig::pull(port))
        } else {
            return None;
        }
    }
}
