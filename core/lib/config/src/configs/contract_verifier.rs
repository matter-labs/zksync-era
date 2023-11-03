// Built-in uses
use std::time::Duration;
// External uses
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ContractVerifierConfig {
    /// Max time of a single compilation (in s).
    pub compilation_timeout: u64,
    /// Interval between polling db for verification requests (in ms).
    pub polling_interval: Option<u64>,
    /// Port to which the Prometheus exporter server is listening.
    pub prometheus_port: u16,
}

impl ContractVerifierConfig {
    pub fn compilation_timeout(&self) -> Duration {
        Duration::from_secs(self.compilation_timeout)
    }

    pub fn polling_interval(&self) -> Duration {
        Duration::from_millis(self.polling_interval.unwrap_or(1000))
    }
}
