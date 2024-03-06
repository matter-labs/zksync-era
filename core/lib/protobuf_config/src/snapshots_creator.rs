use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::required;

use crate::{proto, repr::ProtoRepr};

impl ProtoRepr for proto::SnapshotsCreator {
    type Type = configs::SnapshotsCreatorConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            storage_logs_chunk_size: *required(&self.storage_logs_chunk_size)
                .context("storage_logs_chunk_size")?,
            concurrent_queries_count: *required(&self.concurrent_queries_count)
                .context("concurrent_queries_count")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            storage_logs_chunk_size: Some(this.storage_logs_chunk_size),
            concurrent_queries_count: Some(this.concurrent_queries_count),
        }
    }
}
