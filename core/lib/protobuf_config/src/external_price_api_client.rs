use zksync_config::configs::{self};
use zksync_protobuf::ProtoRepr;

use crate::proto::external_price_api_client as proto;

impl ProtoRepr for proto::ExternalPriceApiClient {
    type Type = configs::external_price_api_client::ExternalPriceApiClientConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(
            configs::external_price_api_client::ExternalPriceApiClientConfig {
                source: self.source.clone().expect("source"),
                client_timeout_ms: self.client_timeout_ms.expect("client_timeout_ms"),
                base_url: self.base_url.clone(),
                api_key: self.api_key.clone(),
                forced_numerator: self.forced_numerator.clone(),
                forced_denominator: self.forced_denominator.clone(),
            },
        )
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            source: Some(this.source.clone()),
            base_url: this.base_url.clone(),
            api_key: this.api_key.clone(),
            client_timeout_ms: Some(this.client_timeout_ms),
            forced_numerator: this.forced_numerator.clone(),
            forced_denominator: this.forced_denominator.clone(),
        }
    }
}
