use anyhow::Context;
use zksync_basic_types::L1BatchNumber;
use zksync_config::configs;
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::vm_runner as proto;

impl ProtoRepr for proto::ProtectiveReadsWriter {
    type Type = configs::ProtectiveReadsWriterConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            db_path: required(&self.db_path).context("db_path")?.clone(),
            window_size: *required(&self.window_size).context("window_size")? as u32,
            first_processed_batch: L1BatchNumber(
                *required(&self.first_processed_batch).context("first_batch")? as u32,
            ),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            db_path: Some(this.db_path.clone()),
            window_size: Some(this.window_size as u64),
            first_processed_batch: Some(this.first_processed_batch.0 as u64),
        }
    }
}

impl ProtoRepr for proto::BasicWitnessInputProducer {
    type Type = configs::BasicWitnessInputProducerConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            db_path: required(&self.db_path).context("db_path")?.clone(),
            window_size: *required(&self.window_size).context("window_size")? as u32,
            first_processed_batch: L1BatchNumber(
                *required(&self.first_processed_batch).context("first_batch")? as u32,
            ),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            db_path: Some(this.db_path.clone()),
            window_size: Some(this.window_size as u64),
            first_processed_batch: Some(this.first_processed_batch.0 as u64),
        }
    }
}
