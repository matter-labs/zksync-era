use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use bigdecimal::ToPrimitive;
use tokio::sync::watch::Receiver;

use zksync_dal::StorageProcessor;
use zksync_web3_decl::{
    jsonrpsee::http_client::{HttpClient, HttpClientBuilder},
    namespaces::ZksNamespaceClient,
};

use super::{gas_adjuster::GasAdjustCoefficient, L1GasPriceProvider};

const SLEEP_INTERVAL: Duration = Duration::from_secs(5);

/// This structure maintains the known L1 gas price by periodically querying
/// the main node.
/// It is required since the main node doesn't only observe the current L1 gas price,
/// but also applies adjustments to it in order to smooth out the spikes.
/// The same algorithm cannot be consistently replicated on the external node side,
/// since it relies on the configuration, which may change.
#[derive(Debug)]
pub struct MainNodeGasPriceFetcher {
    client: HttpClient,
    gas_price: AtomicU64,
    gas_token_adjust_coef: GasAdjustCoefficient,
}

impl MainNodeGasPriceFetcher {
    pub async fn new(main_node_url: &str) -> Self {
        let mut storage = StorageProcessor::establish_connection(true).await;
        let coef = storage.oracle_dal().get_adjust_coefficient().await.unwrap();

        Self {
            client: Self::build_client(main_node_url),
            gas_price: AtomicU64::new(1u64), // Start with 1 wei until the first update.
            gas_token_adjust_coef: GasAdjustCoefficient::new(&coef),
        }
    }

    fn build_client(main_node_url: &str) -> HttpClient {
        HttpClientBuilder::default()
            .build(main_node_url)
            .expect("Unable to create a main node client")
    }

    async fn update_coef(&self) {
        let mut storage = StorageProcessor::establish_connection(true).await;
        let coef = storage.oracle_dal().get_adjust_coefficient().await.unwrap();
        self.gas_token_adjust_coef
            .update_gas_token_adjust_coefficient(&coef);
    }

    pub async fn run(self: Arc<Self>, stop_receiver: Receiver<bool>) {
        loop {
            if *stop_receiver.borrow() {
                vlog::info!("Stop signal received, MainNodeGasPriceFetcher is shutting down");
                break;
            }

            let main_node_gas_price = match self.client.get_l1_gas_price().await {
                Ok(price) => price,
                Err(err) => {
                    vlog::warn!("Unable to get the gas price: {}", err);
                    // A delay to avoid spamming the main node with requests.
                    tokio::time::sleep(SLEEP_INTERVAL).await;
                    continue;
                }
            };
            self.gas_price
                .store(main_node_gas_price.as_u64(), Ordering::Relaxed);

            self.update_coef().await;

            tokio::time::sleep(SLEEP_INTERVAL).await;
        }
    }
}

impl L1GasPriceProvider for MainNodeGasPriceFetcher {
    fn estimate_effective_gas_price(&self) -> u64 {
        let adj_coef = self
            .gas_token_adjust_coef
            .get_gas_token_adjust_coefficient()
            .to_f64()
            .unwrap();
        (self.gas_price.load(Ordering::Relaxed) as f64 * adj_coef) as u64
    }
}
