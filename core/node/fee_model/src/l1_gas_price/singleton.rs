use std::sync::Arc;

use anyhow::Context as _;
use tokio::{sync::watch, task::JoinHandle};
use zksync_base_token_fetcher::ConversionRateFetcher;
use zksync_config::{configs::eth_sender::PubdataSendingMode, GasAdjusterConfig};
use zksync_types::{url::SensitiveUrl, L1ChainId};
use zksync_web3_decl::client::Client;

use super::PubdataPricing;
use crate::l1_gas_price::GasAdjuster;

/// Special struct for creating a singleton of `GasAdjuster`.
/// This is needed only for running the server.
#[derive(Debug)]
pub struct GasAdjusterSingleton {
    chain_id: L1ChainId,
    web3_url: SensitiveUrl,
    gas_adjuster_config: GasAdjusterConfig,
    pubdata_sending_mode: PubdataSendingMode,
    singleton: Option<Arc<GasAdjuster>>,
    base_token_fetcher: Arc<dyn ConversionRateFetcher>,
    pubdata_pricing: Arc<dyn PubdataPricing>,
}

impl GasAdjusterSingleton {
    pub fn new(
        chain_id: L1ChainId,
        web3_url: SensitiveUrl,
        gas_adjuster_config: GasAdjusterConfig,
        pubdata_sending_mode: PubdataSendingMode,
        base_token_fetcher: Arc<dyn ConversionRateFetcher>,
        pubdata_pricing: Arc<dyn PubdataPricing>,
    ) -> Self {
        Self {
            chain_id,
            web3_url,
            gas_adjuster_config,
            pubdata_sending_mode,
            singleton: None,
            base_token_fetcher,
            pubdata_pricing,
        }
    }

    pub async fn get_or_init(&mut self) -> anyhow::Result<Arc<GasAdjuster>> {
        if let Some(adjuster) = &self.singleton {
            Ok(adjuster.clone())
        } else {
            let query_client = Client::http(self.web3_url.clone())
                .context("QueryClient::new()")?
                .for_network(self.chain_id.into())
                .build();
            let adjuster = GasAdjuster::new(
                Box::new(query_client),
                self.gas_adjuster_config,
                self.pubdata_sending_mode,
                self.base_token_fetcher.clone(),
                self.pubdata_pricing.clone(),
            )
            .await
            .context("GasAdjuster::new()")?;

            self.singleton = Some(Arc::new(adjuster));
            Ok(self.singleton.as_ref().unwrap().clone())
        }
    }

    pub fn run_if_initialized(
        self,
        stop_signal: watch::Receiver<bool>,
    ) -> Option<JoinHandle<anyhow::Result<()>>> {
        let gas_adjuster = self.singleton?;
        Some(tokio::spawn(gas_adjuster.run(stop_signal)))
    }
}
