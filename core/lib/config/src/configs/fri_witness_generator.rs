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
}
