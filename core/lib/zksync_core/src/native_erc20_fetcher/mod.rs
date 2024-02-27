use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use metrics::atomics::AtomicU64;
use tokio::{
    sync::{watch, OnceCell},
    task::JoinHandle,
};
use zksync_config::configs::native_erc20_fetcher::NativeErc20FetcherConfig;

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

pub trait Erc20Fetcher: 'static + std::fmt::Debug + Send + Sync {
    fn conversion_rate(&self) -> anyhow::Result<u64>;
}

pub(crate) struct NativeErc20FetcherSingleton {
    native_erc20_fetcher_config: NativeErc20FetcherConfig,
    singleton: OnceCell<Result<Arc<NativeErc20Fetcher>, Error>>,
}

impl NativeErc20FetcherSingleton {
    pub fn new(native_erc20_fetcher_config: NativeErc20FetcherConfig) -> Self {
        Self {
            native_erc20_fetcher_config,
            singleton: OnceCell::new(),
        }
    }

    pub async fn get_or_init(&mut self) -> Result<Arc<NativeErc20Fetcher>, Error> {
        let adjuster = self
            .singleton
            .get_or_init(|| async {
                let fetcher =
                    NativeErc20Fetcher::new(self.native_erc20_fetcher_config.clone()).await;
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

#[derive(Debug)]
pub(crate) struct NativeErc20Fetcher {
    pub config: NativeErc20FetcherConfig,
    pub latest_to_eth_conversion_rate: AtomicU64,
    http_client: reqwest::Client,
}

impl NativeErc20Fetcher {
    pub(crate) async fn new(config: NativeErc20FetcherConfig) -> Self {
        let conversion_rate = reqwest::get(format!("{}/conversion_rate", config.host))
            .await
            .unwrap()
            .json::<u64>()
            .await
            .unwrap();

        let http_client = reqwest::Client::new();

        Self {
            config,
            latest_to_eth_conversion_rate: AtomicU64::new(conversion_rate),
            http_client: http_client,
        }
    }

    pub(crate) async fn run(&self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, native_erc20_fetcher is shutting down");
                break;
            }

            let conversion_rate = self
                .http_client
                .get(format!("{}/conversion_rate", &self.config.host))
                .send()
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
impl Erc20Fetcher for NativeErc20Fetcher {
    fn conversion_rate(&self) -> anyhow::Result<u64> {
        anyhow::Ok(
            self.latest_to_eth_conversion_rate
                .load(std::sync::atomic::Ordering::Relaxed),
        )
    }
}
