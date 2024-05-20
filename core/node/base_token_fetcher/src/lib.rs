use std::{
    cmp::min,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Context;
use async_trait::async_trait;
use bigdecimal::BigDecimal;
use hex::ToHex;
use tokio::time::sleep;
use zksync_config::configs::BaseTokenFetcherConfig;

const MAX_CONVERSION_RATE_FETCH_RETRIES: u8 = 10;

/// Trait used to query the stack's native token conversion rate. Used to properly
/// determine gas prices, as they partially depend on L1 gas prices, denominated in `eth`.
#[async_trait]
pub trait ConversionRateFetcher: 'static + std::fmt::Debug + Send + Sync {
    fn conversion_rate(&self) -> anyhow::Result<BigDecimal>;
    async fn update(&self) -> anyhow::Result<()>;
}

#[derive(Debug)]
pub struct NoOpConversionRateFetcher;
impl Default for NoOpConversionRateFetcher {
    fn default() -> Self {
        Self
    }
}
impl NoOpConversionRateFetcher {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ConversionRateFetcher for NoOpConversionRateFetcher {
    fn conversion_rate(&self) -> anyhow::Result<BigDecimal> {
        Ok(BigDecimal::from(1))
    }

    async fn update(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Struct in charge of periodically querying and caching the native token's conversion rate
/// to `eth`.
#[derive(Debug)]
pub struct BaseTokenFetcher {
    pub config: BaseTokenFetcherConfig,
    pub latest_to_eth_conversion_rate: Arc<Mutex<BigDecimal>>,
    http_client: reqwest::Client,
    error_reporter: Arc<Mutex<ErrorReporter>>,
}

impl BaseTokenFetcher {
    pub async fn new(config: BaseTokenFetcherConfig) -> anyhow::Result<Self> {
        let http_client = reqwest::Client::new();

        let conversion_rate_str = http_client
            .get(format!(
                "{}/conversion_rate/0x{}",
                config.host,
                config.token_address.encode_hex::<String>()
            ))
            .send()
            .await?
            .json::<String>()
            .await
            .context("Unable to parse the response of the native token conversion rate server")?;
        let conversion_rate = BigDecimal::from_str(&conversion_rate_str)
            .context("Unable to parse the response of the native token conversion rate server")?;

        let error_reporter = Arc::new(Mutex::new(ErrorReporter::new()));

        Ok(Self {
            config,
            latest_to_eth_conversion_rate: Arc::new(Mutex::new(conversion_rate)),
            http_client,
            error_reporter,
        })
    }

    /// Attemps to create a new `BaseTokenFetcher` instance with a timeout for getting the initial conversion rate
    pub async fn new_with_timeout(config: BaseTokenFetcherConfig) -> anyhow::Result<Self> {
        let http_client = reqwest::Client::new();

        let mut conversion_rate_str = None;
        let mut tries = 0;
        while conversion_rate_str.is_none() && tries < MAX_CONVERSION_RATE_FETCH_RETRIES {
            match http_client
                .get(format!(
                    "{}/conversion_rate/0x{}",
                    config.host,
                    config.token_address.encode_hex::<String>()
                ))
                .send()
                .await
            {
                Ok(res) => {
                    conversion_rate_str = Some(res.json::<String>().await.context(
                        "Unable to parse the response of the native token conversion rate server",
                    )?);
                }
                Err(_err) => {
                    tries += 1;
                    sleep(Duration::from_secs(1)).await;
                }
            }
        }

        let conversion_rate = conversion_rate_str
            .ok_or_else(|| anyhow::anyhow!("Failed to fetch the native token conversion rate"))?;
        let conversion_rate = BigDecimal::from_str(&conversion_rate)
            .context("Unable to parse the response of the native token conversion rate server")?;
        let error_reporter = Arc::new(Mutex::new(ErrorReporter::new()));
        Ok(Self {
            config,
            latest_to_eth_conversion_rate: Arc::new(Mutex::new(conversion_rate)),
            http_client,
            error_reporter,
        })
    }
}

#[async_trait]
impl ConversionRateFetcher for BaseTokenFetcher {
    fn conversion_rate(&self) -> anyhow::Result<BigDecimal> {
        Ok(self.latest_to_eth_conversion_rate.lock().unwrap().clone())
    }

    async fn update(&self) -> anyhow::Result<()> {
        match self
            .http_client
            .get(format!(
                "{}/conversion_rate/0x{}",
                &self.config.host,
                &self.config.token_address.encode_hex::<String>()
            ))
            .send()
            .await
        {
            Ok(response) => {
                let conversion_rate_str = response.json::<String>().await.context(
                    "Unable to parse the response of the native token conversion rate server",
                )?;
                *self.latest_to_eth_conversion_rate.lock().unwrap() =
                    BigDecimal::from_str(&conversion_rate_str).context(
                        "Unable to parse the response of the native token conversion rate server",
                    )?;

                self.error_reporter.lock().unwrap().reset();
            }
            Err(err) => self
                .error_reporter
                .lock()
                .unwrap()
                .process(anyhow::anyhow!(err)),
        }

        Ok(())
    }
}

#[derive(Debug)]
struct ErrorReporter {
    current_try: u8,
    alert_spawned: bool,
}

impl ErrorReporter {
    const MAX_CONSECUTIVE_NETWORK_ERRORS: u8 = 10;

    fn new() -> Self {
        Self {
            current_try: 0,
            alert_spawned: false,
        }
    }

    fn reset(&mut self) {
        self.current_try = 0;
        self.alert_spawned = false;
    }

    fn process(&mut self, err: anyhow::Error) {
        self.current_try = min(self.current_try + 1, Self::MAX_CONSECUTIVE_NETWORK_ERRORS);

        tracing::error!("Failed to fetch native token conversion rate from the server: {err}");

        if self.current_try >= Self::MAX_CONSECUTIVE_NETWORK_ERRORS && !self.alert_spawned {
            vlog::capture_message(&err.to_string(), vlog::AlertLevel::Warning);
            self.alert_spawned = true;
        }
    }
}
