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
            max_tx_gas: self.max_tx_gas.expect("max_tx_gas"),
            default_priority_fee_per_gas: self
                .default_priority_fee_per_gas
                .expect("default_priority_fee_per_gas"),
            max_acceptable_priority_fee_in_gwei: self
                .max_acceptable_priority_fee_in_gwei
                .expect("max_acceptable_priority_fee_in_gwei"),
            l1_receipt_checking_sleep_ms: self.l1_receipt_checking_sleep_ms,
            l1_receipt_checking_max_attempts: self.l1_receipt_checking_max_attempts,
            l1_tx_sending_max_attempts: self.l1_tx_sending_max_attempts,
            l1_tx_sending_sleep_ms: self.l1_tx_sending_sleep_ms,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            price_polling_interval_ms: Some(this.price_polling_interval_ms),
            price_cache_update_interval_ms: Some(this.price_cache_update_interval_ms),
            l1_receipt_checking_sleep_ms: this.l1_receipt_checking_sleep_ms,
            l1_receipt_checking_max_attempts: this.l1_receipt_checking_max_attempts,
            l1_tx_sending_max_attempts: this.l1_tx_sending_max_attempts,
            l1_tx_sending_sleep_ms: this.l1_tx_sending_sleep_ms,
            max_tx_gas: Some(this.max_tx_gas),
            default_priority_fee_per_gas: Some(this.default_priority_fee_per_gas),
            max_acceptable_priority_fee_in_gwei: Some(this.max_acceptable_priority_fee_in_gwei),
        }
    }
}
