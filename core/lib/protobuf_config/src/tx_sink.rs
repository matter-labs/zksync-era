use zksync_config::configs;
use zksync_protobuf::repr::ProtoRepr;

use crate::proto::tx_sink as proto;

impl ProtoRepr for proto::TxSink {
    type Type = configs::tx_sink::TxSinkConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            deny_list: self.deny_list.clone(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            deny_list: this.deny_list.clone(),
        }
    }
}
