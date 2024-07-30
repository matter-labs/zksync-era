use std::time::Duration;

// Built-in uses
// External uses
use serde::Deserialize;

/// Configuration for the fri witness generation
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct FriWitnessGeneratorConfig {
    /// Max time for witness to be generated
    pub generation_timeout_in_secs: u16,
    pub basic_generation_timeout_in_secs: Option<u16>,
    pub leaf_generation_timeout_in_secs: Option<u16>,
    pub scheduler_generation_timeout_in_secs: Option<u16>,
    pub node_generation_timeout_in_secs: Option<u16>,
    pub recursion_tip_generation_timeout_in_secs: Option<u16>,
    /// Max attempts for generating witness
    pub max_attempts: u32,
    // Optional l1 batch number to process block until(inclusive).
    // This parameter is used in case of performing circuit upgrades(VK/Setup keys),
    // to not let witness-generator pick new job and finish all the existing jobs with old circuit.
    pub last_l1_batch_to_process: Option<u32>,

    // whether to write to public GCS bucket for https://github.com/matter-labs/era-boojum-validator-cli
    pub shall_save_to_public_bucket: bool,

    pub prometheus_listener_port: Option<u16>,

    /// This value corresponds to the maximum number of circuits kept in memory at any given time for a BWG.
    /// Acts as a throttling mechanism for circuits; the trade-off here is speed vs memory usage.
    /// With more circuits in flight, harness does not need to wait for BWG runner to process them.
    /// But every single circuit in flight eats memory (up to 50MB).
    /// WARNING: Do NOT change this value unless you're absolutely sure you know what you're doing.
    /// It affects the performance and resource usage of BWGs.
    #[serde(default = "FriWitnessGeneratorConfig::default_max_circuits_in_flight")]
    pub max_circuits_in_flight: usize,
}

#[derive(Debug)]
pub struct WitnessGenerationTimeouts {
    basic: Duration,
    leaf: Duration,
    node: Duration,
    recursion_tip: Duration,
    scheduler: Duration,
}

impl WitnessGenerationTimeouts {
    pub fn basic(&self) -> Duration {
        self.basic
    }

    pub fn leaf(&self) -> Duration {
        self.leaf
    }

    pub fn node(&self) -> Duration {
        self.node
    }

    pub fn recursion_tip(&self) -> Duration {
        self.recursion_tip
    }

    pub fn scheduler(&self) -> Duration {
        self.scheduler
    }

    pub fn new(basic: u16, leaf: u16, node: u16, recursion_tip: u16, scheduler: u16) -> Self {
        Self {
            basic: Duration::from_secs(basic.into()),
            leaf: Duration::from_secs(leaf.into()),
            node: Duration::from_secs(node.into()),
            recursion_tip: Duration::from_secs(recursion_tip.into()),
            scheduler: Duration::from_secs(scheduler.into()),
        }
    }
}

impl FriWitnessGeneratorConfig {
    pub fn witness_generation_timeouts(&self) -> WitnessGenerationTimeouts {
        WitnessGenerationTimeouts::new(
            self.basic_generation_timeout_in_secs
                .unwrap_or(self.generation_timeout_in_secs),
            self.leaf_generation_timeout_in_secs
                .unwrap_or(self.generation_timeout_in_secs),
            self.node_generation_timeout_in_secs
                .unwrap_or(self.generation_timeout_in_secs),
            self.recursion_tip_generation_timeout_in_secs
                .unwrap_or(self.generation_timeout_in_secs),
            self.scheduler_generation_timeout_in_secs
                .unwrap_or(self.generation_timeout_in_secs),
        )
    }

    pub fn last_l1_batch_to_process(&self) -> u32 {
        self.last_l1_batch_to_process.unwrap_or(u32::MAX)
    }

    // 500 was picked as a mid-ground between allowing enough circuits in flight to speed up circuit generation,
    // whilst keeping memory as low as possible. At the moment, max size of a circuit is ~50MB.
    // This number is important when there are issues with saving circuits (network issues, service unavailability, etc.)
    // Maximum theoretic extra memory consumed is up to 25GB (50MB * 500 circuits), but in reality, worse case scenarios are closer to 5GB (the average space distribution).
    // During normal operations (> P95), this will incur an overhead of ~100MB.
    const fn default_max_circuits_in_flight() -> usize {
        500
    }
}
