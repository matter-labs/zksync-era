use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use metrics::atomics::AtomicU64;
use tokio::sync::watch;
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
#[async_trait::async_trait]
pub trait ConversionRateFetcher: 'static + std::fmt::Debug + Send + Sync {
    fn conversion_rate(&self) -> anyhow::Result<u64>;
    async fn update(&self) -> anyhow::Result<()>;
}

/// Struct in charge of periodically querying and caching the native token's conversion rate
/// to `eth`.
#[derive(Debug)]
pub(crate) struct NativeTokenFetcher {
    pub config: NativeTokenFetcherConfig,
    pub latest_to_eth_conversion_rate: AtomicU64,
}

impl NativeTokenFetcher {
    pub(crate) async fn new(config: NativeTokenFetcherConfig) -> Self {
        let conversion_rate = reqwest::get(format!("{}/conversion_rate", config.host))
            .await
            .unwrap()
            .json::<u64>()
            .await
            .unwrap();

        Self {
            config,
            latest_to_eth_conversion_rate: AtomicU64::new(conversion_rate),
        }
    }

    pub(crate) async fn run(&self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, native_token_fetcher is shutting down");
                break;
            }

            let conversion_rate = reqwest::get(format!("{}/conversion_rate", &self.config.host))
                .await?
                .json::<u64>()
                .await
                .unwrap();

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

    async fn update(&self) -> anyhow::Result<()> {
        let conversion_rate = reqwest::get(format!("{}/conversion_rate", &self.config.host))
            .await?
            .json::<u64>()
            .await
            .unwrap();

        self.latest_to_eth_conversion_rate
            .store(conversion_rate, std::sync::atomic::Ordering::Relaxed);

        Ok(())
    }
}
