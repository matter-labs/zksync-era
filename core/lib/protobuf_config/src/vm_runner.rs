use anyhow::Context;
use zksync_config::configs::{self};
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::vm_runner as proto;

impl ProtoRepr for proto::ProtectiveReadsWriter {
    type Type = configs::ProtectiveReadsWriterConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            protective_reads_db_path: required(&self.protective_reads_db_path)
                .context("protective_reads_db_path")?
                .clone(),
            protective_reads_window_size: *required(&self.protective_reads_window_size)
                .context("protective_reads_window_size")?
                as u32,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            protective_reads_db_path: Some(this.protective_reads_db_path.clone()),
            protective_reads_window_size: Some(this.protective_reads_window_size as u64),
        }
    }
}
