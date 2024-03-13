use std::{
    cmp::min,
    sync::{Arc, Mutex as StdMutex}, // We use Mutex from std to prevent having to await on the lock
};

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use hex::ToHex;
use tokio::sync::Mutex;
use zksync_config::configs::native_token_fetcher::NativeTokenFetcherConfig;
use zksync_dal::BigDecimal;

/// Trait used to query the stack's native token conversion rate. Used to properly
/// determine gas prices, as they partially depend on L1 gas prices, denominated in `eth`.
#[async_trait]
pub trait ConversionRateFetcher: 'static + std::fmt::Debug + Send + Sync {
    fn conversion_rate(&self) -> anyhow::Result<BigDecimal>;
    async fn update(&self) -> anyhow::Result<()>;
}

#[derive(Debug)]
pub(crate) struct NoOpConversionRateFetcher;

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
pub(crate) struct NativeTokenFetcher {
    pub config: NativeTokenFetcherConfig,
    pub latest_to_eth_conversion_rate: Arc<StdMutex<BigDecimal>>,
    http_client: reqwest::Client,
    error_reporter: Arc<Mutex<ErrorReporter>>,
}

impl NativeTokenFetcher {
    pub(crate) async fn new(config: NativeTokenFetcherConfig) -> anyhow::Result<Self> {
        let http_client = reqwest::Client::new();

        let conversion_rate = http_client
            .get(format!(
                "{}/conversion_rate/0x{}",
                config.host,
                config.token_address.encode_hex::<String>()
            ))
            .send()
            .await?
            .json::<BigDecimal>()
            .await
            .context("Unable to parse the response of the native token conversion rate server")?;

        let error_reporter = Arc::new(Mutex::new(ErrorReporter::new()));

        Ok(Self {
            config,
            latest_to_eth_conversion_rate: Arc::new(StdMutex::new(conversion_rate)),
            http_client,
            error_reporter,
        })
    }
}

#[async_trait]
impl ConversionRateFetcher for NativeTokenFetcher {
    fn conversion_rate(&self) -> anyhow::Result<BigDecimal> {
        let lock = match self.latest_to_eth_conversion_rate.lock() {
            Ok(lock) => lock,
            Err(err) => {
                tracing::error!(
                    "Error while getting lock of latest conversion rate: {:?}",
                    err,
                );
                return Err(anyhow!(
                    "Error while getting lock of latest conversion rate: {:?}",
                    err,
                ));
            }
        };
        anyhow::Ok(lock.clone())
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
                let conversion_rate = response.json::<BigDecimal>().await.context(
                    "Unable to parse the response of the native token conversion rate server",
                )?;
                match self.latest_to_eth_conversion_rate.lock() {
                    Ok(mut lock) => {
                        *lock = conversion_rate;
                    }
                    Err(err) => {
                        tracing::error!(
                            "Error while getting lock of latest conversion rate: {:?}",
                            err,
                        );
                        return Err(anyhow!(
                            "Error while getting lock of latest conversion rate: {:?}",
                            err,
                        ));
                    }
                }

                self.error_reporter.lock().await.reset();
            }
            Err(err) => self
                .error_reporter
                .lock()
                .await
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
