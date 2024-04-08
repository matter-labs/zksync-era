use std::{
    sync::{Arc, RwLock},
    time::Duration,
};

use tokio::sync::watch::Receiver;
use zksync_types::fee_model::FeeParams;
use zksync_web3_decl::{client::L2Client, error::ClientRpcContext, namespaces::ZksNamespaceClient};

use crate::fee_model::BatchFeeModelInputProvider;

const SLEEP_INTERVAL: Duration = Duration::from_secs(5);

/// This structure maintains the known L1 gas price by periodically querying
/// the main node.
/// It is required since the main node doesn't only observe the current L1 gas price,
/// but also applies adjustments to it in order to smooth out the spikes.
/// The same algorithm cannot be consistently replicated on the external node side,
/// since it relies on the configuration, which may change.
#[derive(Debug)]
pub struct MainNodeFeeParamsFetcher {
    client: L2Client,
    main_node_fee_params: RwLock<FeeParams>,
}

impl MainNodeFeeParamsFetcher {
    pub fn new(client: L2Client) -> Self {
        Self {
            client: client.for_component("fee_params_fetcher"),
            main_node_fee_params: RwLock::new(FeeParams::sensible_v1_default()),
        }
    }

    pub async fn run(self: Arc<Self>, stop_receiver: Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, MainNodeFeeParamsFetcher is shutting down");
                break;
            }

            let fetch_result = self
                .client
                .get_fee_params()
                .rpc_context("get_fee_params")
                .await;
            let main_node_fee_params = match fetch_result {
                Ok(price) => price,
                Err(err) => {
                    tracing::warn!("Unable to get the gas price: {}", err);
                    // A delay to avoid spamming the main node with requests.
                    tokio::time::sleep(SLEEP_INTERVAL).await;
                    continue;
                }
            };
            *self.main_node_fee_params.write().unwrap() = main_node_fee_params;

            tokio::time::sleep(SLEEP_INTERVAL).await;
        }
        Ok(())
    }
}

impl BatchFeeModelInputProvider for MainNodeFeeParamsFetcher {
    fn get_fee_model_params(&self) -> FeeParams {
        *self.main_node_fee_params.read().unwrap()
    }
}
