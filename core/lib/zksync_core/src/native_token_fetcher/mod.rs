use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use metrics::atomics::AtomicU64;
use tokio::{
    sync::{watch, OnceCell},
    task::JoinHandle,
};
use zksync_config::configs::native_token_fetcher::NativeTokenFetcherConfig;

// TODO: this error type is also defined by the gasAdjuster module,
//       we should consider moving it to a common place
#[derive(thiserror::Error, Debug, Clone)]
#[error(transparent)]
pub struct Error(Arc<anyhow::Error>);

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Self(Arc::new(err))
    }
}

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
    singleton: OnceCell<Result<Arc<NativeTokenFetcher>, Error>>,
}

impl NativeTokenFetcherSingleton {
    pub fn new(native_token_fetcher_config: NativeTokenFetcherConfig) -> Self {
        Self {
            native_token_fetcher_config,
            singleton: OnceCell::new(),
        }
    }

    pub async fn get_or_init(&mut self) -> Result<Arc<NativeTokenFetcher>, Error> {
        let adjuster = self
            .singleton
            .get_or_init(|| async {
                let fetcher =
                    NativeTokenFetcher::new(self.native_token_fetcher_config.clone()).await?;
                Ok(Arc::new(fetcher))
            })
            .await;
        adjuster.clone()
    }

    pub fn run_if_initialized(
        self,
        stop_signal: watch::Receiver<bool>,
    ) -> Option<JoinHandle<anyhow::Result<()>>> {
        let fetcher = self.singleton.get()?.clone();
        Some(tokio::spawn(async move { fetcher?.run(stop_signal).await }))
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
        let conversion_rate = reqwest::get(format!("{}/conversion_rate", config.host))
            .await?
            .json::<u64>()
            .await?;

        let http_client = reqwest::Client::new();

        Ok(Self {
            config,
            latest_to_eth_conversion_rate: AtomicU64::new(conversion_rate),
            http_client: http_client,
        })
    }

    pub(crate) async fn run(&self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, native_token_fetcher is shutting down");
                break;
            }

            let conversion_rate = self
                .http_client
                .get(format!("{}/conversion_rate", &self.config.host))
                .send()
                .await?
                .json::<u64>()
                .await?;

            self.latest_to_eth_conversion_rate
                .store(conversion_rate, std::sync::atomic::Ordering::Relaxed);

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
