//! This module provides several third-party API data fetchers.
//! Examples of fetchers we use:
//!
//! - Token price fetcher, which updates prices for all the tokens we use in zkSync.
//!   Data of this fetcher is used to calculate fees.
//! - Token trading volume fetcher, which updates trading volumes for tokens.
//!   Data of this fetcher is used to decide whether we are going to accept fees in this token.
//!
//! Every data fetcher is represented by an autonomic routine, which spend most of the time sleeping;
//! once in the configurable interval it fetches the data from an API and store it into the database.

use tokio::sync::watch;
use tokio::task::JoinHandle;
use zksync_config::FetcherConfig;
use zksync_dal::ConnectionPool;

pub mod error;
pub mod token_list;
pub mod token_price;

pub fn run_data_fetchers(
    config: &FetcherConfig,
    network: zksync_types::network::Network,
    pool: ConnectionPool,
    stop_receiver: watch::Receiver<bool>,
) -> Vec<JoinHandle<anyhow::Result<()>>> {
    let list_fetcher = token_list::TokenListFetcher::new(config.clone(), network);
    let price_fetcher = token_price::TokenPriceFetcher::new(config.clone());

    vec![
        tokio::spawn(list_fetcher.run(pool.clone(), stop_receiver.clone())),
        tokio::spawn(price_fetcher.run(pool.clone(), stop_receiver.clone())),
    ]
}
