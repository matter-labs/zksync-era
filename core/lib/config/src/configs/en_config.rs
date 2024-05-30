use std::num::{NonZeroU32, NonZeroU64, NonZeroUsize};

use serde::Deserialize;
use zksync_basic_types::{
    commitment::L1BatchCommitmentMode, url::SensitiveUrl, L1ChainId, L2ChainId,
};

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Pruning {
    pub enabled: bool,
    pub chunk_size: Option<u32>,
    pub removal_delay_sec: Option<NonZeroU64>,
    pub data_retention_sec: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SnapshotRecovery {
    pub enabled: bool,
    pub postgres_max_concurrency: Option<NonZeroUsize>,
    pub tree_chunk_size: Option<u64>,
}

/// Temporary config for initializing external node, will be completely replaced by consensus config later
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ENConfig {
    pub l2_chain_id: L2ChainId,
    pub l1_chain_id: L1ChainId,
    pub main_node_url: SensitiveUrl,
    pub commitment_generator_max_parallelism: Option<NonZeroU32>,
    pub l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    pub tree_api_remote_url: Option<String>,
    pub snapshot_recovery: Option<SnapshotRecovery>,
    pub pruning: Option<Pruning>,
}
