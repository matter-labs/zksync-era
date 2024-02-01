use std::{borrow::BorrowMut, collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use metrics::atomics::AtomicU64;
use tokio::{
    sync::{watch, Mutex, OnceCell},
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

#[async_trait]
pub trait Erc20Fetcher: 'static + std::fmt::Debug + Send + Sync {
    async fn conversion_rate(&self) -> anyhow::Result<u64>;
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
                let fetcher = NativeErc20Fetcher::new(self.native_erc20_fetcher_config.clone());
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
    // TODO: we probably need to add a http client here
    // to avoid creating a new one for each request
    pub config: NativeErc20FetcherConfig,
    pub latest_to_eth_conversion_rate: AtomicU64,
}

impl NativeErc20Fetcher {
    pub(crate) fn new(config: NativeErc20FetcherConfig) -> Self {
        Self {
            config,
            latest_to_eth_conversion_rate: AtomicU64::new(0),
        }
    }

    pub(crate) async fn run(&self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, eth_tx_manager is shutting down");
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

    // TODO: implement the actual fetch logic
    pub(crate) async fn fetch_conversion_rate(&self) -> anyhow::Result<u64> {
        Ok(self
            .latest_to_eth_conversion_rate
            .load(std::sync::atomic::Ordering::Relaxed))
    }
}

#[async_trait]
impl Erc20Fetcher for NativeErc20Fetcher {
    async fn conversion_rate(&self) -> anyhow::Result<u64> {
        anyhow::Ok(
            self.latest_to_eth_conversion_rate
                .load(std::sync::atomic::Ordering::Relaxed),
        )
    }
}
