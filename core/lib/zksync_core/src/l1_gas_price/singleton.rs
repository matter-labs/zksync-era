use std::sync::Arc;

use anyhow::Context as _;
use tokio::{
    sync::{watch, OnceCell},
    task::JoinHandle,
};
use zksync_config::GasAdjusterConfig;
use zksync_eth_client::clients::QueryClient;

use crate::{l1_gas_price::GasAdjuster, native_token_fetcher::ConversionRateFetcher};

/// Special struct for creating a singleton of `GasAdjuster`.
/// This is needed only for running the server.
#[derive(Debug)]
pub struct GasAdjusterSingleton {
    web3_url: String,
    gas_adjuster_config: GasAdjusterConfig,
    singleton: OnceCell<anyhow::Result<Arc<GasAdjuster<QueryClient>>>>,
    erc20_fetcher_dyn: Option<Arc<dyn ConversionRateFetcher>>,
}

impl GasAdjusterSingleton {
    pub fn new(
        web3_url: String,
        gas_adjuster_config: GasAdjusterConfig,
        erc20_fetcher_dyn: Option<Arc<dyn ConversionRateFetcher>>,
    ) -> Self {
        Self {
            web3_url,
            gas_adjuster_config,
            singleton: OnceCell::new(),
            erc20_fetcher_dyn,
        }
    }

    pub async fn get_or_init(&mut self) -> anyhow::Result<Arc<GasAdjuster<QueryClient>>> {
        match self
            .singleton
            .get_or_init(|| async {
                let query_client =
                    QueryClient::new(&self.web3_url).context("QueryClient::new()")?;
                let adjuster = GasAdjuster::new(
                    query_client.clone(),
                    self.gas_adjuster_config,
                    self.erc20_fetcher_dyn.clone(),
                )
                .await
                .context("GasAdjuster::new()")?;
                Ok(Arc::new(adjuster))
            })
            .await
        {
            Ok(adjuster) => Ok(adjuster.clone()),
            Err(_e) => Err(anyhow::anyhow!("Failed to get or initialize GasAdjuster")),
        }
    }

    pub fn run_if_initialized(
        self,
        stop_signal: watch::Receiver<bool>,
    ) -> Option<JoinHandle<anyhow::Result<()>>> {
        let gas_adjuster = match self.singleton.get()? {
            Ok(gas_adjuster) => gas_adjuster.clone(),
            Err(_e) => return None,
        };
        Some(tokio::spawn(
            async move { gas_adjuster.run(stop_signal).await },
        ))
    }
}
