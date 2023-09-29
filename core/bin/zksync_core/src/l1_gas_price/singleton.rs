use crate::l1_gas_price::{BoundedGasAdjuster, GasAdjuster};
use std::sync::Arc;
use tokio::sync::{watch, OnceCell};
use tokio::task::JoinHandle;
use zksync_config::{ETHClientConfig, GasAdjusterConfig};
use zksync_eth_client::clients::http::QueryClient;

/// Special struct for creating a singleton of `GasAdjuster`.
/// This is needed only for running the server. This struct uses all configs from env.
#[derive(Debug, Default)]
pub struct GasAdjusterSingleton(OnceCell<Arc<GasAdjuster<QueryClient>>>);

impl GasAdjusterSingleton {
    pub fn new() -> Self {
        Default::default()
    }

    pub async fn get_or_init(&mut self) -> Arc<GasAdjuster<QueryClient>> {
        let adjuster = self
            .0
            .get_or_init(|| async {
                let eth_client_config = ETHClientConfig::from_env();
                let query_client = QueryClient::new(&eth_client_config.web3_url).unwrap();
                let gas_adjuster_config = GasAdjusterConfig::from_env();
                let adjuster = GasAdjuster::new(query_client.clone(), gas_adjuster_config)
                    .await
                    .unwrap();
                Arc::new(adjuster)
            })
            .await;
        adjuster.clone()
    }

    pub async fn get_or_init_bounded(
        &mut self,
    ) -> Arc<BoundedGasAdjuster<GasAdjuster<QueryClient>>> {
        let config = GasAdjusterConfig::from_env();
        let adjuster = self.get_or_init().await;
        Arc::new(BoundedGasAdjuster::new(config.max_l1_gas_price(), adjuster))
    }

    pub fn run_if_initialized(self, stop_signal: watch::Receiver<bool>) -> Option<JoinHandle<()>> {
        self.0
            .get()
            .map(|adjuster| tokio::spawn(adjuster.clone().run(stop_signal)))
    }
}
