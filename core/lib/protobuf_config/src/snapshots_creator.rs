use anyhow::Context as _;
use zksync_basic_types::L1BatchNumber;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto::snapshot_creator as proto;

impl ProtoRepr for proto::SnapshotsCreator {
    type Type = configs::SnapshotsCreatorConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        let object_store = if let Some(object_store) = &self.object_store {
            Some(object_store.read()?)
        } else {
            None
        };
        Ok(Self::Type {
            version: self
                .version
                .unwrap_or_default()
                .try_into()
                .context("version")?,
            l1_batch_number: self.l1_batch_number.map(L1BatchNumber),
            storage_logs_chunk_size: *required(&self.storage_logs_chunk_size)
                .context("storage_logs_chunk_size")?,
            concurrent_queries_count: *required(&self.concurrent_queries_count)
                .context("concurrent_queries_count")?,
            object_store,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            version: Some(this.version.into()),
            l1_batch_number: this.l1_batch_number.map(|num| num.0),
            storage_logs_chunk_size: Some(this.storage_logs_chunk_size),
            concurrent_queries_count: Some(this.concurrent_queries_count),
            object_store: this.object_store.as_ref().map(ProtoRepr::build),
        }
    }
}
