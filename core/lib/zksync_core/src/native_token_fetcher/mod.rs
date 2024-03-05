use std::{
    cmp::min,
    sync::atomic::{AtomicBool, AtomicU8},
};

use anyhow::Context;
use async_trait::async_trait;
use hex::ToHex;
use metrics::atomics::AtomicU64;
use zksync_config::configs::native_token_fetcher::NativeTokenFetcherConfig;

/// Trait used to query the stack's native token conversion rate. Used to properly
/// determine gas prices, as they partially depend on L1 gas prices, denominated in `eth`.
#[async_trait]
pub trait ConversionRateFetcher: 'static + std::fmt::Debug + Send + Sync {
    fn conversion_rate(&self) -> anyhow::Result<u64>;
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
    fn conversion_rate(&self) -> anyhow::Result<u64> {
        Ok(1)
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
    pub latest_to_eth_conversion_rate: AtomicU64,
    http_client: reqwest::Client,
    error_reporter: ErrorReporter,
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
            .json::<u64>()
            .await
            .context("Unable to parse the response of the native token conversion rate server")?;

        let error_reporter = ErrorReporter::new();

        Ok(Self {
            config,
            latest_to_eth_conversion_rate: AtomicU64::new(conversion_rate),
            http_client,
            error_reporter,
        })
    }
}

#[async_trait]
impl ConversionRateFetcher for NativeTokenFetcher {
    fn conversion_rate(&self) -> anyhow::Result<u64> {
        anyhow::Ok(
            self.latest_to_eth_conversion_rate
                .load(std::sync::atomic::Ordering::Relaxed),
        )
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
                let conversion_rate = response.json::<u64>().await.context(
                    "Unable to parse the response of the native token conversion rate server",
                )?;
                self.latest_to_eth_conversion_rate
                    .store(conversion_rate, std::sync::atomic::Ordering::Relaxed);
                self.error_reporter.reset();
            }
            Err(err) => self.error_reporter.process(anyhow::anyhow!(err)),
        }

        Ok(())
    }
}

#[derive(Debug)]
struct ErrorReporter {
    current_try: AtomicU8,
    alert_spawned: AtomicBool,
}

impl ErrorReporter {
    const MAX_CONSECUTIVE_NETWORK_ERRORS: u8 = 10;

    fn new() -> Self {
        Self {
            current_try: AtomicU8::new(0),
            alert_spawned: AtomicBool::new(false),
        }
    }

    fn reset(&self) {
        self.current_try
            .store(0, std::sync::atomic::Ordering::Relaxed);
        self.alert_spawned
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }

    fn process(&self, err: anyhow::Error) {
        let current_try = self.current_try.load(std::sync::atomic::Ordering::Relaxed);
        let new_value = min(current_try + 1, Self::MAX_CONSECUTIVE_NETWORK_ERRORS);
        self.current_try
            .store(new_value, std::sync::atomic::Ordering::Relaxed);

        tracing::error!("Failed to fetch native token conversion rate from the server: {err}");

        let alert_spawned = self
            .alert_spawned
            .load(std::sync::atomic::Ordering::Relaxed);
        if new_value >= Self::MAX_CONSECUTIVE_NETWORK_ERRORS && !alert_spawned {
            vlog::capture_message(&err.to_string(), vlog::AlertLevel::Warning);
            self.alert_spawned
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}
