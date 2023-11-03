use std::time::Duration;

use serde::Deserialize;

/// Configuration for the witness vector generator
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct FriWitnessVectorGeneratorConfig {
    /// Max time before an `reserved` prover instance in considered as `available`
    pub max_prover_reservation_duration_in_secs: u16,
    /// Max time to wait to get a free prover instance
    pub prover_instance_wait_timeout_in_secs: u16,
    // Time to wait between 2 consecutive poll to get new prover instance.
    pub prover_instance_poll_time_in_milli_secs: u16,

    /// Configurations for prometheus
    pub prometheus_listener_port: u16,
    pub prometheus_pushgateway_url: String,
    pub prometheus_push_interval_ms: Option<u64>,

    // specialized group id for this witness vector generator.
    // witness vector generator running the same (circuit id, round) shall have same group id.
    pub specialized_group_id: u8,
}

impl FriWitnessVectorGeneratorConfig {
    pub fn prover_instance_wait_timeout(&self) -> Duration {
        Duration::from_secs(self.prover_instance_wait_timeout_in_secs as u64)
    }

    pub fn prover_instance_poll_time(&self) -> Duration {
        Duration::from_millis(self.prover_instance_poll_time_in_milli_secs as u64)
    }

    pub fn max_prover_reservation_duration(&self) -> Duration {
        Duration::from_secs(self.max_prover_reservation_duration_in_secs as u64)
    }
}
