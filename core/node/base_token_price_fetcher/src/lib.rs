use std::time::Duration;

use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};

#[derive(Debug)]
pub enum TokenPriceInfoSource {
    CoinGecko,
    Custom,
}

#[derive(Debug)]
pub struct BaseTokenPriceFetcherConfig {
    token_price_info_source: TokenPriceInfoSource,
    token_price_info_source_api_key: String,
    token_price_api_token: String,
    poll_interval: u64,
}

// TEMPORARY: REMOVE LATER
impl Default for BaseTokenPriceFetcherConfig {
    fn default() -> Self {
        BaseTokenPriceFetcherConfig {
            token_price_info_source: TokenPriceInfoSource::CoinGecko,
            token_price_info_source_api_key: "".to_string(),
            token_price_api_token: "".to_string(),
            poll_interval: 600,
        }
    }
}

#[derive(Debug)]
pub struct BaseTokenPriceFetcher {
    pub config: BaseTokenPriceFetcherConfig,
    connection_pool: ConnectionPool<Core>,
}

impl BaseTokenPriceFetcher {
    pub fn new(config: BaseTokenPriceFetcherConfig, connection_pool: ConnectionPool<Core>) -> Self {
        BaseTokenPriceFetcher {
            config,
            connection_pool,
        }
    }

    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::debug!("Stopping mempool cache updates");
                return Ok(());
            }

            // Do some work here
            todo!("Base token price fetcher: should fetch and store token price in database");

            tokio::time::sleep(Duration::from_millis(self.config.poll_interval)).await;
        }
    }
}
