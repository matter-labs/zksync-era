use std::{env, time::Duration};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PrometheusConfig {
    /// Port to which the Prometheus exporter server is listening.
    pub listener_port: u16,
    /// URL of the push gateway.
    pub pushgateway_url: Option<String>,
    /// Push interval in ms.
    pub push_interval_ms: Option<u64>,
}

impl PrometheusConfig {
    pub fn push_interval(&self) -> Duration {
        Duration::from_millis(self.push_interval_ms.unwrap_or(100))
    }

    /// Returns the full endpoint URL for the push gateway.
    pub fn gateway_endpoint(&self) -> Option<String> {
        let Some(gateway_url) = &self.pushgateway_url else {
            return None;
        };
        let job_id = "zksync-pushgateway";
        let namespace =
            env::var("POD_NAMESPACE").unwrap_or_else(|_| "UNKNOWN_NAMESPACE".to_owned());
        let pod = env::var("POD_NAME").unwrap_or_else(|_| "UNKNOWN_POD".to_owned());
        Some(format!(
            "{gateway_url}/metrics/job/{job_id}/namespace/{namespace}/pod/{pod}"
        ))
    }
}
