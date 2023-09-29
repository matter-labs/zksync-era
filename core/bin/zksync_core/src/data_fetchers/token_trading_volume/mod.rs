//! Token trading volume fetcher loads the information about how good tokens are being traded on exchanges.
//! We need this information in order to either accept or deny paying fees in a certain tokens:
//! we are only interested in tokens that can be sold to cover expences for the network maintenance.

use std::{collections::HashMap, time::Duration};

use async_trait::async_trait;
use tokio::sync::watch;

use zksync_config::{configs::fetcher::TokenTradingVolumeSource, FetcherConfig};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_types::{tokens::TokenMarketVolume, Address};

use super::error::{ApiFetchError, ErrorAnalyzer};

mod mock;
mod uniswap;

#[async_trait]
pub trait FetcherImpl: std::fmt::Debug + Send + Sync {
    /// Retrieves the list of known tokens.
    async fn fetch_trading_volumes(
        &self,
        tokens: &[Address],
    ) -> Result<HashMap<Address, TokenMarketVolume>, ApiFetchError>;
}

#[derive(Debug)]
pub struct TradingVolumeFetcher {
    config: FetcherConfig,
    fetcher: Box<dyn FetcherImpl>,
    error_handler: ErrorAnalyzer,
}

impl TradingVolumeFetcher {
    fn create_fetcher(config: &FetcherConfig) -> Box<dyn FetcherImpl> {
        let token_trading_volume_config = &config.token_trading_volume;
        match token_trading_volume_config.source {
            TokenTradingVolumeSource::Uniswap => {
                Box::new(uniswap::UniswapTradingVolumeFetcher::new(config)) as Box<dyn FetcherImpl>
            }
            TokenTradingVolumeSource::Mock => {
                Box::new(mock::MockTradingVolumeFetcher::new()) as Box<dyn FetcherImpl>
            }
        }
    }

    pub fn new(config: FetcherConfig) -> Self {
        let fetcher = Self::create_fetcher(&config);
        let error_handler = ErrorAnalyzer::new("TradingVolumeFetcher");
        Self {
            config,
            fetcher,
            error_handler,
        }
    }

    pub async fn run(mut self, pool: ConnectionPool, stop_receiver: watch::Receiver<bool>) {
        let mut fetching_interval =
            tokio::time::interval(self.config.token_trading_volume.fetching_interval());
        loop {
            if *stop_receiver.borrow() {
                vlog::info!("Stop signal received, trading_volume_fetcher is shutting down");
                break;
            }

            fetching_interval.tick().await;
            self.error_handler.update().await;

            let mut storage = pool.access_storage().await;
            let known_l1_tokens = self.load_tokens(&mut storage).await;

            let trading_volumes = match self.fetch_trading_volumes(&known_l1_tokens).await {
                Ok(volumes) => {
                    self.error_handler.reset();
                    volumes
                }
                Err(err) => {
                    self.error_handler.process_error(err);
                    continue;
                }
            };

            self.store_market_volumes(&mut storage, trading_volumes)
                .await;
        }
    }

    async fn fetch_trading_volumes(
        &self,
        addresses: &[Address],
    ) -> Result<HashMap<Address, TokenMarketVolume>, ApiFetchError> {
        const AWAITING_TIMEOUT: Duration = Duration::from_secs(2);

        let fetch_future = self.fetcher.fetch_trading_volumes(addresses);

        tokio::time::timeout(AWAITING_TIMEOUT, fetch_future)
            .await
            .map_err(|_| ApiFetchError::RequestTimeout)?
    }

    async fn store_market_volumes(
        &self,
        storage: &mut StorageProcessor<'_>,
        tokens: HashMap<Address, TokenMarketVolume>,
    ) {
        let mut tokens_dal = storage.tokens_dal();
        for (token, volume) in tokens {
            tokens_dal.set_l1_token_market_volume(&token, volume).await;
        }
    }

    /// Returns the list of tokens with known metadata (if token is not in the list we use,
    /// it's very likely to not have required level of trading volume anyways).
    async fn load_tokens(&self, storage: &mut StorageProcessor<'_>) -> Vec<Address> {
        storage
            .tokens_dal()
            .get_well_known_token_addresses()
            .await
            .into_iter()
            .map(|(l1_token, _)| l1_token)
            .collect()
    }
}
