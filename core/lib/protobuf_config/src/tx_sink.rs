use zksync_config::configs;
use zksync_protobuf::repr::ProtoRepr;

use crate::proto::tx_sink as proto;

impl ProtoRepr for proto::TxSink {
    type Type = configs::tx_sink::TxSinkConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            use_whitelisted_sink: self.use_whitelisted_sink.clone(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            use_whitelisted_sink: this.use_whitelisted_sink.clone(),
        }
    }
}
