use zksync_config::configs::{self};
use zksync_protobuf::ProtoRepr;

use crate::proto::base_token_adjuster as proto;

impl ProtoRepr for proto::BaseTokenAdjuster {
    type Type = configs::base_token_adjuster::BaseTokenAdjusterConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(configs::base_token_adjuster::BaseTokenAdjusterConfig {
            price_polling_interval_ms: self.price_polling_interval_ms,
            base_token: self.base_token.clone(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            price_polling_interval_ms: this.price_polling_interval_ms,
            base_token: this.base_token.clone(),
        }
    }
}
