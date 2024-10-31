use zksync_config::configs::L1RecoveryConfig;
use zksync_protobuf::ProtoRepr;

use crate::proto::l1_recovery as proto;

impl ProtoRepr for proto::L1Recovery {
    type Type = L1RecoveryConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            enabled: self.enabled.unwrap_or_default(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            enabled: Some(this.enabled),
        }
    }
}
