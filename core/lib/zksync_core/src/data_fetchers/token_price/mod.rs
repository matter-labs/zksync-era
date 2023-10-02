//! Token price fetcher is responsible for maintaining actual prices for tokens that are used in zkSync.

use std::{collections::HashMap, time::Duration};

use async_trait::async_trait;

use zksync_config::{configs::fetcher::TokenPriceSource, FetcherConfig};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_types::{tokens::TokenPrice, Address};

use super::error::{ApiFetchError, ErrorAnalyzer};
use bigdecimal::FromPrimitive;
use num::{rational::Ratio, BigUint};
use tokio::sync::watch;

pub mod coingecko;
pub mod mock;

#[async_trait]
pub trait FetcherImpl: std::fmt::Debug + Send + Sync {
    /// Retrieves the token price in USD.
    async fn fetch_token_price(
        &self,
        tokens: &[Address],
    ) -> Result<HashMap<Address, TokenPrice>, ApiFetchError>;
}

#[derive(Debug)]
pub struct TokenPriceFetcher {
    minimum_required_liquidity: Ratio<BigUint>,
    config: FetcherConfig,
    fetcher: Box<dyn FetcherImpl>,
    error_handler: ErrorAnalyzer,
}

impl TokenPriceFetcher {
    fn create_fetcher(config: &FetcherConfig) -> Box<dyn FetcherImpl> {
        let token_price_config = &config.token_price;
        match token_price_config.source {
            TokenPriceSource::CoinGecko => {
                Box::new(coingecko::CoinGeckoFetcher::new(config)) as Box<dyn FetcherImpl>
            }
            TokenPriceSource::CoinMarketCap => {
                unimplemented!()
            }
            TokenPriceSource::Mock => {
                Box::new(mock::MockPriceFetcher::new()) as Box<dyn FetcherImpl>
            }
        }
    }

    pub fn new(config: FetcherConfig) -> Self {
        let fetcher = Self::create_fetcher(&config);
        let error_handler = ErrorAnalyzer::new("TokenPriceFetcher");
        Self {
            minimum_required_liquidity: Ratio::from_integer(
                BigUint::from_u64(0).unwrap(), // We don't use minimum required liquidity in the server anymore.
            ),
            config,
            fetcher,
            error_handler,
        }
    }

    pub async fn run(
        mut self,
        pool: ConnectionPool,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let mut fetching_interval =
            tokio::time::interval(self.config.token_price.fetching_interval());

        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, token_price_fetcher is shutting down");
                break;
            }

            fetching_interval.tick().await;
            self.error_handler.update().await;

            // We refresh token list in case new tokens were added.
            let mut storage = pool.access_storage().await.unwrap();
            let tokens = self.get_tokens(&mut storage).await;

            // Vector of received token prices in the format of (`token_addr`, `price_in_usd`, `fetch_timestamp`).
            let token_prices = match self.fetch_token_price(&tokens).await {
                Ok(prices) => {
                    self.error_handler.reset();
                    prices
                }
                Err(err) => {
                    self.error_handler.process_error(err);
                    continue;
                }
            };
            self.store_token_prices(&mut storage, token_prices).await;
        }
        Ok(())
    }

    async fn fetch_token_price(
        &self,
        tokens: &[Address],
    ) -> Result<HashMap<Address, TokenPrice>, ApiFetchError> {
        const AWAITING_TIMEOUT: Duration = Duration::from_secs(2);

        let fetch_future = self.fetcher.fetch_token_price(tokens);

        tokio::time::timeout(AWAITING_TIMEOUT, fetch_future)
            .await
            .map_err(|_| ApiFetchError::RequestTimeout)?
    }

    async fn store_token_prices(
        &self,
        storage: &mut StorageProcessor<'_>,
        token_prices: HashMap<Address, TokenPrice>,
    ) {
        let mut tokens_dal = storage.tokens_dal();
        for (token, price) in token_prices {
            tokens_dal.set_l1_token_price(&token, price).await;
        }
    }

    /// Returns the list of "interesting" tokens, e.g. ones that can be used to pay fees.
    /// We don't actually need prices for other tokens.
    async fn get_tokens(&self, storage: &mut StorageProcessor<'_>) -> Vec<Address> {
        storage
            .tokens_dal()
            .get_l1_tokens_by_volume(&self.minimum_required_liquidity)
            .await
    }
}
