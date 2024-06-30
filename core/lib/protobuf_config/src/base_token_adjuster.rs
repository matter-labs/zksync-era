use std::num::NonZeroU64;

use anyhow::Context as _;
use zksync_config::configs::{self};
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::base_token_adjuster as proto;

impl ProtoRepr for proto::BaseTokenAdjuster {
    type Type = configs::base_token_adjuster::BaseTokenAdjusterConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(configs::base_token_adjuster::BaseTokenAdjusterConfig {
            price_polling_interval_ms: self.price_polling_interval_ms,
            initial_numerator: NonZeroU64::new(
                *required(&self.initial_numerator).context("Missing initial_numerator")?,
            )
            .expect("initial_numerator must be non-zero"),
            initial_denominator: NonZeroU64::new(
                *required(&self.initial_denominator).context("Missing initial_denominator")?,
            )
            .expect("initial_denominator must be non-zero"),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            price_polling_interval_ms: this.price_polling_interval_ms,
            initial_numerator: Some(this.initial_numerator.get()),
            initial_denominator: Some(this.initial_denominator.get()),
        }
    }
}
