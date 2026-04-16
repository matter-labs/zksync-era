use std::{
    sync::{Arc, RwLock},
    time::Duration,
};

use async_trait::async_trait;
use tokio::sync::watch::Receiver;
use zksync_types::{
    fee_model::{BatchFeeInput, FeeParams},
    U256,
};
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::ClientRpcContext,
    jsonrpsee,
    namespaces::{EnNamespaceClient, ZksNamespaceClient},
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
    main_node_fee_state: RwLock<MainNodeFeeState>,
}

#[derive(Debug, Clone, Copy)]
struct MainNodeFeeState {
    fee_params: FeeParams,
    fee_input: BatchFeeInput,
    interop_fee: U256,
}

impl MainNodeFeeParamsFetcher {
    pub fn new(client: Box<DynClient<L2>>) -> Self {
        let fee_params = FeeParams::sensible_v1_default();
        let fee_input = fee_params.scale(1.0, 1.0);
        Self {
            client: client.for_component("fee_params_fetcher"),
            main_node_fee_state: RwLock::new(MainNodeFeeState {
                fee_params,
                fee_input,
                // Interop fee is zero until first successful poll
                interop_fee: U256::zero(),
            }),
        }
    }

    pub async fn run(self: Arc<Self>, mut stop_receiver: Receiver<bool>) -> anyhow::Result<()> {
        while !*stop_receiver.borrow_and_update() {
            // We query fee params and fee input together to minimize the potential for them to be
            // out of sync. They can still be fetched out of sync in rare circumstances but nothing
            // in the system *directly* relies on `BatchFeeModelInputProvider::get_fee_model_params`
            // except for `zks_getFeeParams`. Which is likely fine because EN is essentially
            // mimicking how it observed the call to main node.
            let (params_result, input_result, interop_fee_result) = tokio::join!(
                self.client.get_fee_params().rpc_context("get_fee_params"),
                self.client
                    .get_batch_fee_input()
                    .rpc_context("get_batch_fee_input"),
                self.client.get_interop_fee().rpc_context("get_interop_fee"),
            );
            {
                let mut main_node_fee_state = self.main_node_fee_state.write().unwrap();

                match params_result.and_then(|params| input_result.map(|input| (params, input))) {
                    Ok((fee_params, fee_input)) => {
                        main_node_fee_state.fee_params = fee_params;
                        main_node_fee_state.fee_input =
                            BatchFeeInput::PubdataIndependent(fee_input);
                    }
                    Err(err) => {
                        tracing::warn!("Unable to get main node's fee params/input: {}", err);
                    }
                }

                match interop_fee_result {
                    Ok(interop_fee) => {
                        main_node_fee_state.interop_fee = interop_fee;
                    }
                    Err(err) => match err.as_ref() {
                        jsonrpsee::core::client::Error::Call(error)
                            if error.code() == jsonrpsee::types::error::METHOD_NOT_FOUND_CODE =>
                        {
                            // Method is not supported by the main node, preserve the previously observed value.
                        }
                        _ => {
                            tracing::warn!("Unable to get main node's interop fee: {}", err);
                        }
                    },
                }
            }

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
        Ok(self.main_node_fee_state.read().unwrap().fee_input)
    }

    async fn get_fee_model_params(&self) -> FeeParams {
        self.main_node_fee_state.read().unwrap().fee_params
    }

    async fn get_interop_fee(&self) -> U256 {
        self.main_node_fee_state.read().unwrap().interop_fee
    }
}
