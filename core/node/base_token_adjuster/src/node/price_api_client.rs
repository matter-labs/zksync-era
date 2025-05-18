use std::{str::FromStr, sync::Arc};

use zksync_config::configs::ExternalPriceApiClientConfig;
use zksync_external_price_api::{
    cmc_api::CmcPriceApiClient, coingecko_api::CoinGeckoPriceAPIClient,
    forced_price_client::ForcedPriceClient, NoOpPriceAPIClient,
};
use zksync_node_framework::{IntoContext, WiringError, WiringLayer};

use super::resources::PriceAPIClientResource;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, Default)]
enum ExternalPriceApiKind {
    #[default]
    NoOp,
    Forced,
    CoinGecko,
    CoinMarketCap,
}

impl FromStr for ExternalPriceApiKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match &s.to_lowercase()[..] {
            "no-op" | "noop" => Self::NoOp,
            "forced" => Self::Forced,
            "coingecko" => Self::CoinGecko,
            "coinmarketcap" => Self::CoinMarketCap,
            _ => anyhow::bail!("Unknown external price API client source: {s:?}"),
        })
    }
}

impl ExternalPriceApiKind {
    fn instantiate(&self, config: ExternalPriceApiClientConfig) -> PriceAPIClientResource {
        PriceAPIClientResource(match self {
            Self::NoOp => Arc::new(NoOpPriceAPIClient),
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
    type Error = anyhow::Error;

    fn try_from(config: ExternalPriceApiClientConfig) -> Result<Self, Self::Error> {
        Ok(Self {
            kind: config.source.parse()?,
            config,
        })
    }
}

#[derive(Debug, IntoContext)]
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
