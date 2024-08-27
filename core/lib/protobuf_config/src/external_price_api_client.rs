use zksync_config::configs::{self, external_price_api_client::ForcedPriceClientConfig};
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
                forced: Some(ForcedPriceClientConfig {
                    numerator: self.forced_numerator,
                    denominator: self.forced_denominator,
                    fluctuation: self.forced_fluctuation,
                }),
            },
        )
    }

    fn build(this: &Self::Type) -> Self {
        let numerator = this.forced.as_ref().and_then(|x| x.numerator);
        let denominator = this.forced.as_ref().and_then(|x| x.denominator);
        let fluctuation = this.forced.as_ref().and_then(|x| x.fluctuation);

        Self {
            source: Some(this.source.clone()),
            base_url: this.base_url.clone(),
            api_key: this.api_key.clone(),
            client_timeout_ms: Some(this.client_timeout_ms),
            forced_numerator: numerator,
            forced_denominator: denominator,
            forced_fluctuation: fluctuation,
        }
    }
}
