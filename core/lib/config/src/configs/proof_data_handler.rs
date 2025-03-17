use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ProofDataHandlerConfig {
    pub http_port: u16,
    pub api_url: String,
    pub batch_readiness_check_interval_in_secs: u16,
    pub proof_generation_timeout_in_secs: u16,
    pub retry_connection_interval_in_secs: u16,
}

impl ProofDataHandlerConfig {
    pub fn proof_generation_timeout(&self) -> Duration {
        Duration::from_secs(self.proof_generation_timeout_in_secs as u64)
    }
    pub fn batch_readiness_check_interval(&self) -> Duration {
        Duration::from_secs(self.batch_readiness_check_interval_in_secs as u64)
    }

    pub fn retry_connection_interval(&self) -> Duration {
        Duration::from_secs(self.retry_connection_interval_in_secs as u64)
    }
}
