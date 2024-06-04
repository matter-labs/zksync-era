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
            token_price_info_source: TokenPriceInfoSource::Custom,
            token_price_api_token: "0x0d8775f648430679a709e98d2b0cb6250d2887ef".to_string(),
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

            let conversion_rate = self.get_price().await?;

            let token_price_data = TokenPriceData {
                token: Address::from_str(&self.config.token_price_api_token)
                    .expect("Invalid address"),
                rate: conversion_rate,
                timestamp: 1, // TODO: replace with actual timestamp
            };

            connection
                .token_price_dal()
                .insert_ratio(token_price_data)
                .await?;

            tokio::time::sleep(Duration::from_millis(self.config.poll_interval)).await;
        }
    }
}
