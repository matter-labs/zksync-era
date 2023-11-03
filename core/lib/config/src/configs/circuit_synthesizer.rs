use std::time::Duration;

use serde::Deserialize;

/// Configuration for the witness generation
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct CircuitSynthesizerConfig {
    /// Max time for circuit to be synthesized
    pub generation_timeout_in_secs: u16,
    /// Max attempts for synthesizing circuit
    pub max_attempts: u32,
    /// Max time before an `reserved` prover instance in considered as `available`
    pub gpu_prover_queue_timeout_in_secs: u16,
    /// Max time to wait to get a free prover instance
    pub prover_instance_wait_timeout_in_secs: u16,
    // Time to wait between 2 consecutive poll to get new prover instance.
    pub prover_instance_poll_time_in_milli_secs: u16,
    /// Configurations for prometheus
    pub prometheus_listener_port: u16,
    pub prometheus_pushgateway_url: String,
    pub prometheus_push_interval_ms: Option<u64>,
    // Group id for this synthesizer, synthesizer running the same circuit types shall have same group id.
    pub prover_group_id: u8,
}

impl CircuitSynthesizerConfig {
    pub fn generation_timeout(&self) -> Duration {
        Duration::from_secs(self.generation_timeout_in_secs as u64)
    }

    pub fn prover_instance_wait_timeout(&self) -> Duration {
        Duration::from_secs(self.prover_instance_wait_timeout_in_secs as u64)
    }

    pub fn gpu_prover_queue_timeout(&self) -> Duration {
        Duration::from_secs(self.gpu_prover_queue_timeout_in_secs as u64)
    }

    pub fn prover_instance_poll_time(&self) -> Duration {
        Duration::from_millis(self.prover_instance_poll_time_in_milli_secs as u64)
    }
}
