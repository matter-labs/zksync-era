use std::num::NonZeroUsize;

use anyhow::Context;
use zksync_basic_types::L1BatchNumber;
use zksync_config::configs::{
    snapshot_recovery::{Postgres, Tree},
    SnapshotRecoveryConfig,
};
use zksync_protobuf::ProtoRepr;

use crate::{proto::snapshot_recovery as proto, read_optional_repr};

impl ProtoRepr for proto::Tree {
    type Type = Tree;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            chunk_size: self.chunk_size,
            parallel_persistence_buffer: self
                .parallel_persistence_buffer
                .and_then(|a| NonZeroUsize::new(a as usize)),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            chunk_size: this.chunk_size,
            parallel_persistence_buffer: this.parallel_persistence_buffer.map(|a| a.get() as u64),
        }
    }
}

impl ProtoRepr for proto::Postgres {
    type Type = Postgres;

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
        Ok(Self::Type {
            enabled: self.enabled.unwrap_or_default(),
            tree: read_optional_repr(&self.tree)
                .context("tree")?
                .unwrap_or_default(),
            postgres: read_optional_repr(&self.postgres)
                .context("postgres")?
                .unwrap_or_default(),
            l1_batch: self.l1_batch.map(L1BatchNumber),
        })
    }

    fn build(this: &Self::Type) -> Self {
        let tree = if this.tree == Tree::default() {
            None
        } else {
            Some(this.tree.clone())
        };
        let postgres = if this.postgres == Postgres::default() {
            None
        } else {
            Some(this.postgres.clone())
        };
        Self {
            enabled: Some(this.enabled),
            postgres: postgres.as_ref().map(ProtoRepr::build),
            tree: tree.as_ref().map(ProtoRepr::build),
            l1_batch: this.l1_batch.map(|a| a.0),
        }
    }
}
