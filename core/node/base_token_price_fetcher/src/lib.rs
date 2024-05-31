use std::{str::FromStr, time::Duration};

use anyhow::Context;
use tokio::sync::watch;
use zksync_basic_types::Address;
use zksync_dal::{BigDecimal, ConnectionPool, Core, CoreDal};
use zksync_types::tokens::TokenPriceData;

#[derive(Debug)]
pub enum TokenPriceInfoSource {
    CoinGecko,
    Custom,
}

impl TokenPriceInfoSource {
    pub fn host(&self) -> &'static str {
        match self {
            TokenPriceInfoSource::CoinGecko => "https://api.coingecko.com/api/v3",
            TokenPriceInfoSource::Custom => panic!("Custom source does not have a host"), // TODO: what should be done here?
        }
    }
}

#[derive(Debug)]
pub struct BaseTokenPriceFetcherConfig {
    token_price_info_source: TokenPriceInfoSource,
    token_price_api_token: String,
    poll_interval: u64,
}

// TEMPORARY: REMOVE LATER
impl Default for BaseTokenPriceFetcherConfig {
    fn default() -> Self {
        BaseTokenPriceFetcherConfig {
            token_price_info_source: TokenPriceInfoSource::CoinGecko,
            token_price_api_token: "".to_string(),
            poll_interval: 600,
        }
    }
}

#[derive(Debug)]
pub struct BaseTokenPriceFetcher {
    pub config: BaseTokenPriceFetcherConfig,
    connection_pool: ConnectionPool<Core>,
    http_client: reqwest::Client,
}

impl BaseTokenPriceFetcher {
    pub fn new(config: BaseTokenPriceFetcherConfig, connection_pool: ConnectionPool<Core>) -> Self {
        BaseTokenPriceFetcher {
            config,
            connection_pool,
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut connection = self.connection_pool.connection().await.unwrap();
        loop {
            if *stop_receiver.borrow() {
                tracing::debug!("Stopping mempool cache updates");
                return Ok(());
            }

            // TODO: parsing depends on the TokenPriceInfoSource we are using
            // so we need to implement this fetch based on each source
            let conversion_rate_str = self
                .http_client
                .get(format!(
                    "{}/conversion_rate/0x{}",
                    self.config.token_price_info_source.host(),
                    self.config.token_price_api_token,
                ))
                .send()
                .await?
                .json::<String>()
                .await
                .context(
                    "Unable to parse the response of the native token conversion rate server",
                )?;

            let conversion_rate = BigDecimal::from_str(&conversion_rate_str)
                .context("Unable to parse the conversion rate")?;

            let token_price_data = TokenPriceData {
                token: Address::from_str(&self.config.token_price_api_token)
                    .expect("Invalid address"),
                rate: conversion_rate,
                timestamp: 1, // TODO: replace with actual timestamp
            };
            //
            //

            connection
                .token_price_dal()
                .insert_ratio(token_price_data)
                .await?;

            tokio::time::sleep(Duration::from_millis(self.config.poll_interval)).await;
        }
    }
}
