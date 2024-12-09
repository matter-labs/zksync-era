use std::time::Duration;

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

/// Configuration for the witness vector generator
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct FriWitnessVectorGeneratorConfig {
    /// Max time before an `reserved` prover instance in considered as `available`
    #[config(default_t = Duration::from_secs(1_000), with = TimeUnit::Seconds)]
    pub max_prover_reservation_duration_in_secs: Duration,
    /// Max time to wait to get a free prover instance
    #[config(default_t = Duration::from_secs(200), with = TimeUnit::Seconds)]
    pub prover_instance_wait_timeout_in_secs: Duration,
    /// Time to wait between 2 consecutive poll to get new prover instance.
    #[config(default_t = Duration::from_millis(250), with = TimeUnit::Millis)]
    pub prover_instance_poll_time_in_milli_secs: Duration,

    // Configurations for prometheus
    pub prometheus_listener_port: u16,

    /// Specialized group id for this witness vector generator.
    /// Witness vector generator running the same (circuit id, round) shall have same group id.
    pub specialized_group_id: u8,
}
