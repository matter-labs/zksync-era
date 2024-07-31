use zksync_config::configs::{self};
use zksync_protobuf::ProtoRepr;

use crate::proto::base_token_adjuster as proto;

impl ProtoRepr for proto::BaseTokenAdjuster {
    type Type = configs::base_token_adjuster::BaseTokenAdjusterConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(configs::base_token_adjuster::BaseTokenAdjusterConfig {
            price_polling_interval_ms: self
                .price_polling_interval_ms
                .expect("price_polling_interval_ms"),

            price_cache_update_interval_ms: self
                .price_cache_update_interval_ms
                .expect("price_cache_update_interval_ms"),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            price_polling_interval_ms: Some(this.price_polling_interval_ms),
            price_cache_update_interval_ms: Some(this.price_cache_update_interval_ms),
        }
    }
}
