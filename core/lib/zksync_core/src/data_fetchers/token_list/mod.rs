//! Token list fetcher is an entity capable of receiving information about token symbols, decimals, etc.
//!
//! Since we accept manual token addition to zkSync, we must be aware of some scam-tokens that are trying
//! to pretend to be something else. This is why we don't rely on the information that is provided by
//! the token smart contract itself.
//! Instead, we analyze somewhat truthful information source to pick the list of relevant tokens.
//!
//! If requested token is not in the list provided by the API, it's symbol will be displayed as
//! "ERC20-{token_address}" and decimals will be set to 18 (default value for most of the tokens).

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use async_trait::async_trait;
use tokio::sync::watch;

use zksync_config::{configs::fetcher::TokenListSource, FetcherConfig};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_types::network::Network;
use zksync_types::{tokens::TokenMetadata, Address};

use super::error::{ApiFetchError, ErrorAnalyzer};

mod mock;
mod one_inch;

#[async_trait]
pub trait FetcherImpl: std::fmt::Debug + Send + Sync {
    /// Retrieves the list of known tokens.
    async fn fetch_token_list(&self) -> Result<HashMap<Address, TokenMetadata>, ApiFetchError>;
}

#[derive(Debug)]
pub struct TokenListFetcher {
    config: FetcherConfig,
    fetcher: Box<dyn FetcherImpl>,
    error_handler: ErrorAnalyzer,
}

impl TokenListFetcher {
    fn create_fetcher(config: &FetcherConfig, network: Network) -> Box<dyn FetcherImpl> {
        let token_list_config = &config.token_list;
        match token_list_config.source {
            TokenListSource::OneInch => {
                Box::new(one_inch::OneInchTokenListFetcher::new(config)) as Box<dyn FetcherImpl>
            }
            TokenListSource::Mock => {
                Box::new(mock::MockTokenListFetcher::new(network)) as Box<dyn FetcherImpl>
            }
        }
    }

    pub fn new(config: FetcherConfig, network: Network) -> Self {
        let fetcher = Self::create_fetcher(&config, network);
        let error_handler = ErrorAnalyzer::new("TokenListFetcher");
        Self {
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
            tokio::time::interval(self.config.token_list.fetching_interval());

        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, token_list_fetcher is shutting down");
                break;
            }

            fetching_interval.tick().await;
            self.error_handler.update().await;

            let mut token_list = match self.fetch_token_list().await {
                Ok(list) => {
                    self.error_handler.reset();
                    list
                }
                Err(err) => {
                    self.error_handler.process_error(err);
                    continue;
                }
            };

            // We assume that token metadata does not change, thus we only looking for the new tokens.
            let mut storage = pool.access_storage().await.unwrap();
            let unknown_tokens = self.load_unknown_tokens(&mut storage).await;
            token_list.retain(|token, _data| unknown_tokens.contains(token));

            self.update_tokens(&mut storage, token_list).await;
        }
        Ok(())
    }

    async fn fetch_token_list(&self) -> Result<HashMap<Address, TokenMetadata>, ApiFetchError> {
        const AWAITING_TIMEOUT: Duration = Duration::from_secs(2);

        let fetch_future = self.fetcher.fetch_token_list();

        tokio::time::timeout(AWAITING_TIMEOUT, fetch_future)
            .await
            .map_err(|_| ApiFetchError::RequestTimeout)?
    }

    async fn update_tokens(
        &self,
        storage: &mut StorageProcessor<'_>,
        tokens: HashMap<Address, TokenMetadata>,
    ) {
        let mut tokens_dal = storage.tokens_dal();
        for (token, metadata) in tokens {
            tokens_dal
                .update_well_known_l1_token(&token, metadata)
                .await;
        }
    }

    async fn load_unknown_tokens(&self, storage: &mut StorageProcessor<'_>) -> HashSet<Address> {
        storage
            .tokens_dal()
            .get_unknown_l1_token_addresses()
            .await
            .into_iter()
            .collect()
    }
}
