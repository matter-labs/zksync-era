use std::num::NonZeroU64;

use zksync_config::configs::PruningConfig;
use zksync_protobuf::ProtoRepr;

use crate::proto::pruning as proto;

impl ProtoRepr for proto::Pruning {
    type Type = PruningConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            enabled: self.enabled.unwrap_or_default(),
            chunk_size: self.chunk_size,
            removal_delay_sec: self.removal_delay_sec.and_then(NonZeroU64::new),
            data_retention_sec: self.data_retention_sec,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            enabled: Some(this.enabled),
            chunk_size: this.chunk_size,
            removal_delay_sec: this.removal_delay_sec.map(|a| a.get()),
            data_retention_sec: this.data_retention_sec,
        }
    }
}
