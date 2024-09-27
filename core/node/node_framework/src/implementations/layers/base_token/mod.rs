use std::{str::FromStr, sync::Arc};

use zksync_config::configs::ExternalPriceApiClientConfig;
use zksync_external_price_api::{
    cmc_api::CmcPriceApiClient, coingecko_api::CoinGeckoPriceAPIClient,
    forced_price_client::ForcedPriceClient, NoOpPriceAPIClient,
};

use crate::{
    implementations::resources::price_api_client::PriceAPIClientResource, IntoContext, WiringError,
    WiringLayer,
};

pub mod base_token_ratio_persister;
pub mod base_token_ratio_provider;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, Default)]
enum ExternalPriceApiKind {
    #[default]
    NoOp,
    Forced,
    CoinGecko,
    CoinMarketCap,
}

#[derive(Debug, thiserror::Error)]
#[error("Unknown external price API client source: \"{0}\"")]
pub struct UnknownExternalPriceApiClientSourceError(String);

impl FromStr for ExternalPriceApiKind {
    type Err = UnknownExternalPriceApiClientSourceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match &s.to_lowercase()[..] {
            "no-op" | "noop" => Self::NoOp,
            "forced" => Self::Forced,
            "coingecko" => Self::CoinGecko,
            "coinmarketcap" => Self::CoinMarketCap,
            _ => return Err(UnknownExternalPriceApiClientSourceError(s.to_owned())),
        })
    }
}

impl ExternalPriceApiKind {
    fn instantiate(&self, config: ExternalPriceApiClientConfig) -> PriceAPIClientResource {
        PriceAPIClientResource(match self {
            Self::NoOp => Arc::new(NoOpPriceAPIClient {}),
            Self::Forced => Arc::new(ForcedPriceClient::new(config)),
            Self::CoinGecko => Arc::new(CoinGeckoPriceAPIClient::new(config)),
            Self::CoinMarketCap => Arc::new(CmcPriceApiClient::new(config)),
        })
    }
}

#[derive(Debug)]
pub struct ExternalPriceApiLayer {
    kind: ExternalPriceApiKind,
    config: ExternalPriceApiClientConfig,
}

impl TryFrom<ExternalPriceApiClientConfig> for ExternalPriceApiLayer {
    type Error = UnknownExternalPriceApiClientSourceError;

    fn try_from(config: ExternalPriceApiClientConfig) -> Result<Self, Self::Error> {
        Ok(Self {
            kind: config.source.parse()?,
            config,
        })
    }
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub price_api_client: PriceAPIClientResource,
}

#[async_trait::async_trait]
impl WiringLayer for ExternalPriceApiLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "external_price_api"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        Ok(Output {
            price_api_client: self.kind.instantiate(self.config),
        })
    }
}
