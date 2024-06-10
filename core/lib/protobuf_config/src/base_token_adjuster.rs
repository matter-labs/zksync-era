use zksync_config::configs::{self};
use zksync_protobuf::ProtoRepr;

use crate::proto::base_token_adjuster as proto;

impl ProtoRepr for proto::BaseTokenAdjuster {
    type Type = configs::base_token_adjuster::BaseTokenAdjusterConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(configs::base_token_adjuster::BaseTokenAdjusterConfig {
            polling_interval_ms: self.polling_interval,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            polling_interval: this.polling_interval_ms,
        }
    }
}
