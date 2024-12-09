use std::{path::PathBuf, time::Duration};

use serde::Deserialize;
use smart_config::{
    de::{Optional, Serde, WellKnown},
    metadata::TimeUnit,
    DescribeConfig, DeserializeConfig,
};

use crate::ObjectStoreConfig;

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Default)]
pub enum SetupLoadMode {
    #[default]
    FromDisk,
    FromMemory,
}

impl WellKnown for SetupLoadMode {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
}

/// Kind of cloud environment prover subsystem runs in.
///
/// Currently will only affect how the prover zone is chosen.
#[derive(Debug, Default, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum CloudConnectionMode {
    /// Assumes that the prover runs in GCP.
    /// Will use zone information to make sure that the direct network communication
    /// between components is performed only within the same zone.
    #[default]
    GCP,
    /// Assumes that the prover subsystem runs locally.
    Local,
}

impl WellKnown for CloudConnectionMode {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
}

/// Configuration for the fri prover application
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct FriProverConfig {
    pub setup_data_path: PathBuf,
    #[config(default_t = 3315)]
    pub prometheus_port: u16,
    #[config(default_t = 5)]
    pub max_attempts: u32,
    #[config(default_t = Duration::from_secs(600), with = TimeUnit::Seconds)]
    pub generation_timeout_in_secs: Duration,
    #[config(default)]
    pub setup_load_mode: SetupLoadMode,
    pub specialized_group_id: u8,
    #[config(default_t = 10)]
    pub queue_capacity: usize,
    #[config(default_t = 3316)]
    pub witness_vector_receiver_port: u16,
    pub zone_read_url: String,
    #[config(with = Optional(TimeUnit::Seconds))]
    pub availability_check_interval_in_secs: Option<Duration>,
    /// whether to write to public GCS bucket for https://github.com/matter-labs/era-boojum-validator-cli
    #[config(default)]
    pub shall_save_to_public_bucket: bool,
    #[config(default)]
    pub cloud_type: CloudConnectionMode,

    #[config(nest)]
    pub prover_object_store: ObjectStoreConfig,
    #[config(nest)]
    pub public_object_store: ObjectStoreConfig,
}
