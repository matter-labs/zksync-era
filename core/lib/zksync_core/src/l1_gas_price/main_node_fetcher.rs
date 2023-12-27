use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};

use tokio::sync::watch::Receiver;
use zksync_system_constants::{GAS_PER_PUBDATA_BYTE, L1_GAS_PER_PUBDATA_BYTE};
use zksync_types::fee_model::FeeModelOutput;
use zksync_web3_decl::{
    jsonrpsee::http_client::{HttpClient, HttpClientBuilder},
    namespaces::ZksNamespaceClient,
};

use super::L1GasPriceProvider;
use crate::fee_model::FeeBatchInputProvider;

const SLEEP_INTERVAL: Duration = Duration::from_secs(5);

/// This structure maintains the known L1 gas price by periodically querying
/// the main node.
/// It is required since the main node doesn't only observe the current L1 gas price,
/// but also applies adjustments to it in order to smooth out the spikes.
/// The same algorithm cannot be consistently replicated on the external node side,
/// since it relies on the configuration, which may change.
#[derive(Debug)]
pub struct MainNodeFeeParamsFetcher {
    client: HttpClient,
    fee_model_output: RwLock<FeeModelOutput>,
}

impl MainNodeFeeParamsFetcher {
    pub fn new(main_node_url: &str) -> Self {
        Self {
            client: Self::build_client(main_node_url),
            fee_model_output: RwLock::new(FeeModelOutput {
                l1_gas_price: 1_000_000_000,
                fair_pubdata_price: 17_000_000_000,
                fair_l2_gas_price: 1_000_000_000,
            }),
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

            let main_node_fee_params = match self.client.get_main_node_fee_params().await {
                Ok(price) => price,
                Err(err) => {
                    tracing::warn!("Unable to get the gas price: {}", err);
                    // A delay to avoid spamming the main node with requests.
                    tokio::time::sleep(SLEEP_INTERVAL).await;
                    continue;
                }
            };

            *self.fee_model_output.write().unwrap() = main_node_fee_params;

            tokio::time::sleep(SLEEP_INTERVAL).await;
        }
        Ok(())
    }
}

impl FeeBatchInputProvider for MainNodeFeeParamsFetcher {
    // TODO: use scale l1 gas prices
    fn get_fee_model_params(&self, scale_l1_prices: bool) -> FeeModelOutput {
        self.fee_model_output.read().unwrap().clone()
    }
}
