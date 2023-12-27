use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::sync::watch::Receiver;
use zksync_system_constants::{GAS_PER_PUBDATA_BYTE, L1_GAS_PER_PUBDATA_BYTE};
use zksync_web3_decl::{
    jsonrpsee::http_client::{HttpClient, HttpClientBuilder},
    namespaces::ZksNamespaceClient,
};

use super::L1GasPriceProvider;

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
    l1_gas_price: AtomicU64,
    l1_pubdata_price: AtomicU64,
    l2_fair_gas_price: AtomicU64,
}

impl MainNodeGasPriceFetcher {
    pub fn new(main_node_url: &str) -> Self {
        Self {
            client: Self::build_client(main_node_url),
            l1_gas_price: AtomicU64::new(1u64),
            // Start with 1 wei until the first update.
            l1_pubdata_price: AtomicU64::new(1u64),
            // todo: fix it
            l2_fair_gas_price: AtomicU64::new(100000000u64),
        }
    }

    fn build_client(main_node_url: &str) -> HttpClient {
        HttpClientBuilder::default()
            .build(main_node_url)
            .expect("Unable to create a main node client")
    }

    pub async fn run(self: Arc<Self>, stop_receiver: Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, MainNodeGasPriceFetcher is shutting down");
                break;
            }

            let main_node_gas_price = match self.client.get_l1_gas_price().await {
                Ok(price) => price,
                Err(err) => {
                    tracing::warn!("Unable to get the gas price: {}", err);
                    // A delay to avoid spamming the main node with requests.
                    tokio::time::sleep(SLEEP_INTERVAL).await;
                    continue;
                }
            };
            self.l1_gas_price
                .store(main_node_gas_price.as_u64(), Ordering::Relaxed);
            self.l1_pubdata_price
                .store(main_node_gas_price.as_u64() * 17, Ordering::Relaxed);
            tokio::time::sleep(SLEEP_INTERVAL).await;
        }
        Ok(())
    }
}

impl L1GasPriceProvider for MainNodeGasPriceFetcher {
    fn estimate_effective_gas_price(&self) -> u64 {
        self.l1_gas_price.load(Ordering::Relaxed)
    }

    fn estimate_effective_pubdata_price(&self) -> u64 {
        self.l1_pubdata_price.load(Ordering::Relaxed)
    }
}
