use std::num::NonZeroU64;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PruningConfig {
    pub enabled: bool,
    pub chunk_size: Option<u32>,
    pub removal_delay_sec: Option<NonZeroU64>,
    pub data_retention_sec: Option<u64>,
}
