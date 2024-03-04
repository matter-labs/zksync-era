use std::{cmp::min, sync::Arc, time::Duration};

use anyhow::Context;
use async_trait::async_trait;
use hex::ToHex;
use metrics::atomics::AtomicU64;
use tokio::{
    sync::{watch, OnceCell},
    task::JoinHandle,
};
use zksync_config::configs::native_token_fetcher::NativeTokenFetcherConfig;

/// Trait used to query the stack's native token conversion rate. Used to properly
/// determine gas prices, as they partially depend on L1 gas prices, denominated in `eth`.
pub trait ConversionRateFetcher: 'static + std::fmt::Debug + Send + Sync {
    fn conversion_rate(&self) -> anyhow::Result<u64>;
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
}

pub(crate) struct NativeTokenFetcherSingleton {
    native_token_fetcher_config: NativeTokenFetcherConfig,
    singleton: OnceCell<anyhow::Result<Arc<NativeTokenFetcher>>>,
}

impl NativeTokenFetcherSingleton {
    pub fn new(native_token_fetcher_config: NativeTokenFetcherConfig) -> Self {
        Self {
            native_token_fetcher_config,
            singleton: OnceCell::new(),
        }
    }

    pub async fn get_or_init(&mut self) -> anyhow::Result<Arc<NativeTokenFetcher>> {
        match self
            .singleton
            .get_or_init(|| async {
                Ok(Arc::new(
                    NativeTokenFetcher::new(self.native_token_fetcher_config.clone()).await?,
                ))
            })
            .await
        {
            Ok(fetcher) => Ok(fetcher.clone()),
            Err(_e) => Err(anyhow::anyhow!(
                "Failed to get or initialize NativeTokenFetcher"
            )),
        }
    }

    pub fn run_if_initialized(
        self,
        stop_signal: watch::Receiver<bool>,
    ) -> Option<JoinHandle<anyhow::Result<()>>> {
        let fetcher = match self.singleton.get()? {
            Ok(fetcher) => fetcher.clone(),
            Err(_e) => return None,
        };
        Some(tokio::spawn(async move { fetcher.run(stop_signal).await }))
    }
}

/// Struct in charge of periodically querying and caching the native token's conversion rate
/// to `eth`.
#[derive(Debug)]
pub(crate) struct NativeTokenFetcher {
    pub config: NativeTokenFetcherConfig,
    pub latest_to_eth_conversion_rate: AtomicU64,
    http_client: reqwest::Client,
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

        Ok(Self {
            config,
            latest_to_eth_conversion_rate: AtomicU64::new(conversion_rate),
            http_client,
        })
    }

    pub(crate) async fn run(&self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut error_reporter = ErrorReporter::new();
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, native_token_fetcher is shutting down");
                break;
            }

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
                    error_reporter.reset();
                }
                Err(err) => error_reporter.process(anyhow::anyhow!(err)),
            }

            tokio::time::sleep(Duration::from_secs(self.config.poll_interval)).await;
        }

        Ok(())
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
