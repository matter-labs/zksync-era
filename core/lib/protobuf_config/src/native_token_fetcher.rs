use anyhow::{Context as _, Ok};
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::{parse_h160, proto::native_token_fetcher as proto};

impl ProtoRepr for proto::NativeTokenFetcher {
    type Type = configs::NativeTokenFetcherConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            poll_interval: *required(&self.poll_interval).context("poll_interval")?,
            host: required(&self.host).context("host")?.clone(),
            token_address: required(&self.token_address)
                .and_then(|a| parse_h160(a))
                .context("fee_account_addr")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            poll_interval: Some(this.poll_interval),
            host: Some(this.host.clone()),
            token_address: Some(this.token_address.as_bytes().into()),
        }
    }
}
