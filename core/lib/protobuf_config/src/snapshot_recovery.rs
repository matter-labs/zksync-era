use std::num::NonZeroUsize;

use zksync_basic_types::L1BatchNumber;
use zksync_config::configs::SnapshotRecoveryConfig;
use zksync_protobuf::ProtoRepr;

use crate::proto::snapshot_recovery as proto;

impl ProtoRepr for proto::SnapshotRecovery {
    type Type = SnapshotRecoveryConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            enabled: self.enabled.unwrap_or_default(),
            postgres_max_concurrency: self
                .postgres_max_concurrency
                .and_then(|a| NonZeroUsize::new(a as usize)),
            tree_chunk_size: self.tree_chunk_size,
            tree_parallel_persistence_buffer: self
                .tree_parallel_persistence_buffer
                .and_then(|a| NonZeroUsize::new(a as usize)),

            l1_batch: self.l1_batch.map(L1BatchNumber),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            enabled: Some(this.enabled),
            postgres_max_concurrency: this.postgres_max_concurrency.map(|a| a.get() as u64),
            tree_chunk_size: this.tree_chunk_size,
            l1_batch: this.l1_batch.map(|a| a.0),
            tree_parallel_persistence_buffer: this
                .tree_parallel_persistence_buffer
                .map(|a| a.get() as u64),
        }
    }
}
