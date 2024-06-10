use anyhow::{Context as _, Ok};
use zksync_config::configs::BaseTokenConfig;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::{parse_h160, proto::base_token as proto};

impl ProtoRepr for proto::BaseToken {
    type Type = BaseTokenConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            base_token_address: required(&self.base_token_address)
                .and_then(|a| parse_h160(a))
                .context("base_token_address")?,
            outdated_token_price_timeout: required(&self.outdated_token_price_timeout)
                .and_then(|t| Ok(Some(t.clone())))
                .context("outdated_token_price_timeout")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            base_token_address: Some(this.base_token_address.to_string()),
            outdated_token_price_timeout: this.outdated_token_price_timeout,
        }
    }
}
