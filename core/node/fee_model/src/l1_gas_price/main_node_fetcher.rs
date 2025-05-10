use std::{
    sync::{Arc, RwLock},
    time::Duration,
};

use async_trait::async_trait;
use tokio::sync::watch::Receiver;
use zksync_types::fee_model::{BatchFeeInput, FeeParams};
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::ClientRpcContext,
    namespaces::ZksNamespaceClient,
};

use crate::BatchFeeModelInputProvider;

const SLEEP_INTERVAL: Duration = Duration::from_secs(5);

/// This structure maintains the known fee params/input by periodically querying
/// the main node.
///
/// It is required since the main node doesn't only observe the current L1 gas price,
/// but also applies adjustments to it in order to smooth out the spikes.
/// The same algorithm cannot be consistently replicated on the external node side,
/// since it relies on the configuration, which may change.
#[derive(Debug)]
pub struct MainNodeFeeParamsFetcher {
    client: Box<DynClient<L2>>,
    main_node_fee_state: RwLock<(FeeParams, BatchFeeInput)>,
}

impl MainNodeFeeParamsFetcher {
    pub fn new(client: Box<DynClient<L2>>) -> Self {
        let fee_params = FeeParams::sensible_v1_default();
        let fee_input = fee_params.scale(1.0, 1.0);
        Self {
            client: client.for_component("fee_params_fetcher"),
            main_node_fee_state: RwLock::new((fee_params, fee_input)),
        }
    }

    pub async fn run(self: Arc<Self>, mut stop_receiver: Receiver<bool>) -> anyhow::Result<()> {
        while !*stop_receiver.borrow_and_update() {
            // We query fee params and fee input together to minimize the potential for them to be
            // out of sync. They can still be fetched out of sync in rare circumstances but nothing
            // in the system *directly* relies on `BatchFeeModelInputProvider::get_fee_model_params`
            // except for `zks_getFeeParams`. Which is likely fine because EN is essentially
            // mimicking how it observed the call to main node.
            let (params_result, input_result) = tokio::join!(
                self.client.get_fee_params().rpc_context("get_fee_params"),
                self.client
                    .get_batch_fee_input()
                    .rpc_context("get_batch_fee_input")
            );
            let fee_state_result =
                params_result.and_then(|params| input_result.map(|input| (params, input)));
            let main_node_fee_state = match fee_state_result {
                Ok((fee_params, fee_input)) => {
                    (fee_params, BatchFeeInput::PubdataIndependent(fee_input))
                }
                Err(err) => {
                    tracing::warn!("Unable to get main node's fee params/input: {}", err);
                    // A delay to avoid spamming the main node with requests.
                    if tokio::time::timeout(SLEEP_INTERVAL, stop_receiver.changed())
                        .await
                        .is_ok()
                    {
                        break;
                    }
                    continue;
                }
            };
            *self.main_node_fee_state.write().unwrap() = main_node_fee_state;

            if tokio::time::timeout(SLEEP_INTERVAL, stop_receiver.changed())
                .await
                .is_ok()
            {
                break;
            }
        }

        tracing::info!("Stop request received, MainNodeFeeParamsFetcher is shutting down");
        Ok(())
    }
}

#[async_trait]
impl BatchFeeModelInputProvider for MainNodeFeeParamsFetcher {
    async fn get_batch_fee_input_scaled(
        &self,
        // EN's scale factors are ignored as we have already fetched scaled fee input from main node
        _l1_gas_price_scale_factor: f64,
        _l1_pubdata_price_scale_factor: f64,
    ) -> anyhow::Result<BatchFeeInput> {
        Ok(self.main_node_fee_state.read().unwrap().1)
    }

    fn get_fee_model_params(&self) -> FeeParams {
        self.main_node_fee_state.read().unwrap().0
    }
}
