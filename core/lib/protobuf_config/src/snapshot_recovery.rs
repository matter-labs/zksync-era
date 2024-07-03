use std::num::NonZeroUsize;

use anyhow::Context;
use zksync_basic_types::L1BatchNumber;
use zksync_config::configs::{
    snapshot_recovery::{PostgresRecoveryConfig, TreeRecoveryConfig},
    SnapshotRecoveryConfig,
};
use zksync_protobuf::ProtoRepr;

use crate::{proto::snapshot_recovery as proto, read_optional_repr};

impl ProtoRepr for proto::Postgres {
    type Type = PostgresRecoveryConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            max_concurrency: self
                .max_concurrency
                .and_then(|a| NonZeroUsize::new(a as usize)),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            max_concurrency: this.max_concurrency.map(|a| a.get() as u64),
        }
    }
}

impl ProtoRepr for proto::SnapshotRecovery {
    type Type = SnapshotRecoveryConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        let tree = self
            .tree
            .as_ref()
            .map(|tree| {
                let chunk_size = tree.chunk_size;
                let parallel_persistence_buffer = self
                    .experimental
                    .as_ref()
                    .and_then(|a| {
                        a.tree_recovery_parallel_persistence_buffer
                            .map(|a| NonZeroUsize::new(a as usize))
                    })
                    .flatten();
                TreeRecoveryConfig {
                    chunk_size,
                    parallel_persistence_buffer,
                }
            })
            .unwrap_or_default();

        Ok(Self::Type {
            enabled: self.enabled.unwrap_or_default(),
            tree,
            postgres: read_optional_repr(&self.postgres)
                .context("postgres")?
                .unwrap_or_default(),
            l1_batch: self.l1_batch.map(L1BatchNumber),
            object_store: read_optional_repr(&self.object_store).context("object store")?,
            drop_storage_key_preimages: self
                .experimental
                .as_ref()
                .and_then(|experimental| experimental.drop_storage_key_preimages)
                .unwrap_or_default(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        let (tree, experimental) = if this.tree == TreeRecoveryConfig::default() {
            (None, None)
        } else {
            (
                Some(proto::Tree {
                    chunk_size: this.tree.chunk_size,
                }),
                Some(crate::proto::experimental::SnapshotRecovery {
                    tree_recovery_parallel_persistence_buffer: this
                        .tree
                        .parallel_persistence_buffer
                        .map(|a| a.get() as u64),
                    drop_storage_key_preimages: Some(this.drop_storage_key_preimages),
                }),
            )
        };
        let postgres = if this.postgres == PostgresRecoveryConfig::default() {
            None
        } else {
            Some(this.postgres.clone())
        };
        Self {
            enabled: Some(this.enabled),
            postgres: postgres.as_ref().map(ProtoRepr::build),
            tree,
            experimental,
            l1_batch: this.l1_batch.map(|a| a.0),
            object_store: this.object_store.as_ref().map(ProtoRepr::build),
        }
    }
}
