use zksync_config::configs;
use zksync_protobuf::repr::ProtoRepr;

use crate::proto::tx_sink as proto;

impl ProtoRepr for proto::TxSink {
    type Type = configs::tx_sink::TxSinkConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            deployment_allowlist_sink: self.deployment_allowlist_sink.unwrap_or_default(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            deployment_allowlist_sink: Some(this.deployment_allowlist_sink),
        }
    }
}
