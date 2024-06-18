use std::time::Duration;

use tokio::sync::watch;
use zksync_basic_types::Address;
use zksync_dal::{BigDecimal, ConnectionPool, Core, CoreDal};
use zksync_types::tokens::TokenPriceData;

#[derive(Debug, Clone)]
pub enum TokenPriceInfoSource {
    CoinGecko,
    Custom,
}

#[derive(Debug, Clone)]
pub struct BaseTokenPriceFetcherConfig {
    pub token_price_info_source: TokenPriceInfoSource,
    pub token_price_api_token: Address,
    pub poll_interval: u64,
}

impl Default for BaseTokenPriceFetcherConfig {
    fn default() -> Self {
        BaseTokenPriceFetcherConfig {
            token_price_info_source: TokenPriceInfoSource::Custom,
            token_price_api_token: Address::default(),
            poll_interval: 600,
        }
    }
}

#[derive(Debug)]
pub struct BaseTokenPriceFetcher {
    pub config: BaseTokenPriceFetcherConfig,
    connection_pool: ConnectionPool<Core>,
    _http_client: reqwest::Client,
}

impl BaseTokenPriceFetcher {
    pub fn new(config: BaseTokenPriceFetcherConfig, connection_pool: ConnectionPool<Core>) -> Self {
        BaseTokenPriceFetcher {
            config,
            connection_pool,
            _http_client: reqwest::Client::new(),
        }
    }

    pub async fn get_price(&self) -> anyhow::Result<BigDecimal> {
        match self.config.token_price_info_source {
            TokenPriceInfoSource::Custom => Ok(BigDecimal::from(1)),
            _ => {
                todo!("BaseTokenPriceFetcher::get_price")
            }
        }
    }

    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut connection = self.connection_pool.connection().await.unwrap();
        loop {
            if *stop_receiver.borrow() {
                tracing::debug!("Stopping mempool cache updates");
                return Ok(());
            }

            let token_price_data = TokenPriceData {
                address: self.config.token_price_api_token,
                rate: self.get_price().await?,
            };

            connection
                .token_price_dal()
                .insert_ratio(token_price_data)
                .await?;

            tokio::time::sleep(Duration::from_millis(self.config.poll_interval)).await;
        }
    }
}
